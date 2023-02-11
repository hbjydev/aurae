/* -------------------------------------------------------------------------- *\
 *             Apache 2.0 License Copyright © 2022-2023 The Aurae Authors          *
 *                                                                            *
 *                +--------------------------------------------+              *
 *                |   █████╗ ██╗   ██╗██████╗  █████╗ ███████╗ |              *
 *                |  ██╔══██╗██║   ██║██╔══██╗██╔══██╗██╔════╝ |              *
 *                |  ███████║██║   ██║██████╔╝███████║█████╗   |              *
 *                |  ██╔══██║██║   ██║██╔══██╗██╔══██║██╔══╝   |              *
 *                |  ██║  ██║╚██████╔╝██║  ██║██║  ██║███████╗ |              *
 *                |  ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝ |              *
 *                +--------------------------------------------+              *
 *                                                                            *
 *                         Distributed Systems Runtime                        *
 *                                                                            *
 * -------------------------------------------------------------------------- *
 *                                                                            *
 *   Licensed under the Apache License, Version 2.0 (the "License");          *
 *   you may not use this file except in compliance with the License.         *
 *   You may obtain a copy of the License at                                  *
 *                                                                            *
 *       http://www.apache.org/licenses/LICENSE-2.0                           *
 *                                                                            *
 *   Unless required by applicable law or agreed to in writing, software      *
 *   distributed under the License is distributed on an "AS IS" BASIS,        *
 *   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. *
 *   See the License for the specific language governing permissions and      *
 *   limitations under the License.                                           *
 *                                                                            *
\* -------------------------------------------------------------------------- */

#[allow(unused_imports)]
use crate::cri::oci::AuraeOCIBuilder;
use crate::spawn_auraed_oci_to;
use libcontainer;
use libcontainer::{
    container::builder::ContainerBuilder,
    syscall::syscall::create_syscall,
};
use proto::cri::{
    runtime_service_server, AttachRequest, AttachResponse,
    CheckpointContainerRequest, CheckpointContainerResponse,
    ContainerEventResponse, ContainerStatsRequest, ContainerStatsResponse,
    ContainerStatusRequest, ContainerStatusResponse,
    CreateContainerRequest, CreateContainerResponse, ExecRequest, ExecResponse,
    ExecSyncRequest, ExecSyncResponse, GetEventsRequest,
    ListContainerStatsRequest, ListContainerStatsResponse,
    ListContainersRequest, ListContainersResponse,
    ListMetricDescriptorsRequest, ListMetricDescriptorsResponse,
    ListPodSandboxMetricsRequest, ListPodSandboxMetricsResponse,
    ListPodSandboxRequest, ListPodSandboxResponse, ListPodSandboxStatsRequest,
    ListPodSandboxStatsResponse, PodSandbox, PodSandboxStatsRequest,
    PodSandboxStatsResponse, PodSandboxStatusRequest, PodSandboxStatusResponse,
    PortForwardRequest, PortForwardResponse, RemoveContainerRequest,
    RemoveContainerResponse, RemovePodSandboxRequest, RemovePodSandboxResponse,
    ReopenContainerLogRequest, ReopenContainerLogResponse,
    RunPodSandboxRequest, RunPodSandboxResponse, StartContainerRequest,
    StartContainerResponse, StatusRequest, StatusResponse,
    StopContainerRequest, StopContainerResponse, StopPodSandboxRequest,
    StopPodSandboxResponse, UpdateContainerResourcesRequest,
    UpdateContainerResourcesResponse, UpdateRuntimeConfigRequest,
    UpdateRuntimeConfigResponse, VersionRequest, VersionResponse,
};
use chrono::Utc;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::syscall::syscall::create_syscall;
use nix::sys::signal::Signal;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;
use std::path::Path;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::{error::RuntimeServiceError, sandbox_cache::SandboxCache};

// The string to refer to the nested runtime spaces for recursive Auraed environments.
const AURAE_SELF_IDENTIFIER: &str = "_aurae";

// Top level path for all Aurae runtime pod state
const AURAE_PODS_PATH: &str = "/var/run/aurae/pods";

// Specific path for the Aurae spawn OCI bundle
const AURAE_BUNDLE_PATH: &str = "/var/run/aurae/bundles";

#[derive(Debug, Clone)]
pub struct RuntimeService {
    sandboxes: Arc<Mutex<SandboxCache>>,
}

impl RuntimeService {
    pub fn new() -> Self {
        RuntimeService { sandboxes: Default::default() }
    }
}

#[tonic::async_trait]
impl runtime_service_server::RuntimeService for RuntimeService {
    async fn version(
        &self,
        _request: Request<VersionRequest>,
    ) -> Result<Response<VersionResponse>, Status> {
        todo!()
    }

    /// Run a pod with the Aurae runtime daemon.
    async fn run_pod_sandbox(
        &self,
        request: Request<RunPodSandboxRequest>,
    ) -> Result<Response<RunPodSandboxResponse>, Status> {
        // TODO: RuntimeServiceErrors

        // Handle Request
        let r = request.into_inner();
        // Handle Config
        let config = r.config.expect("config from pod sandbox request");
        // Check for Windows config (currently unsupported)
        let windows = config.clone().windows;
        if windows.is_some() {
            panic!("Windows architecture is currently unsupported.") // TODO Unsure if we want to panic here?
        }

        let mut sandboxes = self.sandboxes.lock().await;

        // Extract the metadata (name, uid, etc)
        let metadata = config.clone().metadata.expect("metadata from config");
        let sandbox_id = metadata.name;
        // Extract the Linux config (OCI and runtime parameters, security context, etc)
        let _linux =
            config.clone().linux.expect("linux from pod sandbox config");
        let oci_builder =
            AuraeOCIBuilder::new().overload_pod_sandbox_config(config);

        // TODO Switch on "KernelSpec" which is a field that we will add to the RunPodSandboxRequest message
        // TODO Switch on KernelSpec (if exists) and toggle between "VM Mode" and "Container Mode"
        // TODO Switch on "WASM" which is a field that we will add to the RunPodSandboxRequest
        // TODO We made the decision to create a "KernelSpec" *name structure that will be how we distinguish between VMs and Containers

        let sandbox = {
            // Initialize a new container builder with the AURAE_SELF_IDENTIFIER name as the "init" container running a recursive Auraed
            let syscall = create_syscall();
            let sandbox_builder = ContainerBuilder::new(
                AURAE_SELF_IDENTIFIER.to_string(),
                syscall.as_ref(),
            );

            // Spawn auraed here
            // TODO Check if sandbox already exists and if it does, clean up to this point?
            let _spawned = spawn_auraed_oci_to(
                Path::new(AURAE_BUNDLE_PATH)
                    .join(AURAE_SELF_IDENTIFIER)
                    .to_str()
                    .expect("recursive path"),
                oci_builder.build().expect("building pod oci spec"),
            );

            let mut sandbox = Some(sandbox_builder
                .with_root_path(Path::new(AURAE_PODS_PATH).join(sandbox_id.clone()))
                .expect("Setting pods directory")
                .as_init(Path::new(AURAE_BUNDLE_PATH).join(AURAE_SELF_IDENTIFIER))
                .with_systemd(false)
                .build()
                .expect("failed building pod sandbox: ensure valid OCI spec and proper container starting point"));

            sandbox
                .as_mut()
                .expect("a sandbox")
                .start()
                .expect("starting pod sandbox");
            sandbox
        };

        // NOTE: we're creating a libcontainer::container and calling it `sandbox`,
        // so that's what goes in the cache.  but i think the rest of this service
        // differentiates between the outer "pod" and the inner "container"s so we
        // might want to do the same.
        sandboxes.add(sandbox_id.clone(), sandbox.expect("a sandbox"))?;

        Ok(Response::new(RunPodSandboxResponse { pod_sandbox_id: sandbox_id }))
    }

    async fn stop_pod_sandbox(
        &self,
        request: Request<StopPodSandboxRequest>,
    ) -> Result<Response<StopPodSandboxResponse>, Status> {
        let sandbox_id = request.into_inner().pod_sandbox_id;

        let mut sandboxes = self.sandboxes.lock().await;
        let sandbox = sandboxes.get_mut(&sandbox_id)?;
        sandbox.kill(Signal::SIGKILL, false).map_err(|e| {
            RuntimeServiceError::KillError { sandbox_id, error: e.to_string() }
        })?;
        Ok(Response::new(StopPodSandboxResponse {}))
    }

    async fn remove_pod_sandbox(
        &self,
        request: Request<RemovePodSandboxRequest>,
    ) -> Result<Response<RemovePodSandboxResponse>, Status> {
        let sandbox_id = request.into_inner().pod_sandbox_id;
        let mut sandboxes = self.sandboxes.lock().await;
        if sandboxes.get(&sandbox_id)?.status() != libcontainer::container::ContainerStatus::Stopped {
            return Err(RuntimeServiceError::SandboxNotExited {sandbox_id }.into());
        }
        sandboxes.remove(&sandbox_id)?;
        Ok(Response::new(RemovePodSandboxResponse {}))
    }

    async fn pod_sandbox_status(
        &self,
        request: Request<PodSandboxStatusRequest>,
    ) -> Result<Response<PodSandboxStatusResponse>, Status> {
        let sandbox_id = request.into_inner().pod_sandbox_id;
        let sandboxes = self.sandboxes.lock().await;
        let state = sandboxes.get(&sandbox_id)?.status();
        // FIXME: this needs to be mapped more correctly.
        let container_status = aurae_proto::cri::ContainerStatus {
            id: sandbox_id,
            state: state as i32,

            ..Default::default()
        };
        Ok(Response::new(PodSandboxStatusResponse {
            status: None,
            info: Default::default(),
            containers_statuses: vec![container_status],
            timestamp: Utc::now().timestamp(),
        }))
    }

    async fn list_pod_sandbox(
        &self,
        _request: Request<ListPodSandboxRequest>,
    ) -> Result<Response<ListPodSandboxResponse>, Status> {
        // TODO: filter
        let sandboxes = self.sandboxes.lock().await;
        let all_sandboxes = sandboxes.list()?;
        Ok(Response::new(ListPodSandboxResponse {
            items: all_sandboxes
                .iter()
                .map(|s| PodSandbox {
                    // TODO: other fields
                    id: s.id().to_string(),
                    ..Default::default()
                })
                .collect(),
        }))
    }

    async fn create_container(
        &self,
        request: Request<CreateContainerRequest>,
    ) -> Result<Response<CreateContainerResponse>, Status> {
        // Handle Request
        let r = request.into_inner();
        // Handle Config
        let config = r.config.expect("config from create container request");
        // Metadata
        let metadata = config.metadata.expect("metadata from config");

        // TODO: Pull sandbox from cache

        let syscall = create_syscall();
        let _sandbox_builder =
            ContainerBuilder::new(metadata.name, syscall.as_ref());

        // TODO schedule as tenant container

        todo!()
    }

    async fn start_container(
        &self,
        _request: Request<StartContainerRequest>,
    ) -> Result<Response<StartContainerResponse>, Status> {
        todo!()
    }

    async fn stop_container(
        &self,
        _request: Request<StopContainerRequest>,
    ) -> Result<Response<StopContainerResponse>, Status> {
        todo!()
    }

    async fn remove_container(
        &self,
        _request: Request<RemoveContainerRequest>,
    ) -> Result<Response<RemoveContainerResponse>, Status> {
        todo!()
    }

    async fn list_containers(
        &self,
        _request: Request<ListContainersRequest>,
    ) -> Result<Response<ListContainersResponse>, Status> {
        todo!()
    }

    async fn container_status(
        &self,
        _request: Request<ContainerStatusRequest>,
    ) -> Result<Response<ContainerStatusResponse>, Status> {
        todo!()
    }

    async fn update_container_resources(
        &self,
        _request: Request<UpdateContainerResourcesRequest>,
    ) -> Result<Response<UpdateContainerResourcesResponse>, Status> {
        todo!()
    }

    async fn reopen_container_log(
        &self,
        _request: Request<ReopenContainerLogRequest>,
    ) -> Result<Response<ReopenContainerLogResponse>, Status> {
        todo!()
    }

    async fn exec_sync(
        &self,
        _request: Request<ExecSyncRequest>,
    ) -> Result<Response<ExecSyncResponse>, Status> {
        todo!()
    }

    async fn exec(
        &self,
        _request: Request<ExecRequest>,
    ) -> Result<Response<ExecResponse>, Status> {
        todo!()
    }

    async fn attach(
        &self,
        _request: Request<AttachRequest>,
    ) -> Result<Response<AttachResponse>, Status> {
        todo!()
    }

    async fn port_forward(
        &self,
        _request: Request<PortForwardRequest>,
    ) -> Result<Response<PortForwardResponse>, Status> {
        todo!()
    }

    async fn container_stats(
        &self,
        _request: Request<ContainerStatsRequest>,
    ) -> Result<Response<ContainerStatsResponse>, Status> {
        todo!()
    }

    async fn list_container_stats(
        &self,
        _request: Request<ListContainerStatsRequest>,
    ) -> Result<Response<ListContainerStatsResponse>, Status> {
        todo!()
    }

    async fn pod_sandbox_stats(
        &self,
        _request: Request<PodSandboxStatsRequest>,
    ) -> Result<Response<PodSandboxStatsResponse>, Status> {
        todo!()
    }

    async fn list_pod_sandbox_stats(
        &self,
        _request: Request<ListPodSandboxStatsRequest>,
    ) -> Result<Response<ListPodSandboxStatsResponse>, Status> {
        todo!()
    }

    async fn update_runtime_config(
        &self,
        _request: Request<UpdateRuntimeConfigRequest>,
    ) -> Result<Response<UpdateRuntimeConfigResponse>, Status> {
        todo!()
    }

    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        todo!()
    }

    async fn checkpoint_container(
        &self,
        _request: Request<CheckpointContainerRequest>,
    ) -> Result<Response<CheckpointContainerResponse>, Status> {
        todo!()
    }

    type GetContainerEventsStream =
        ReceiverStream<Result<ContainerEventResponse, Status>>;

    async fn get_container_events(
        &self,
        _request: Request<GetEventsRequest>,
    ) -> Result<Response<Self::GetContainerEventsStream>, Status> {
        todo!()
    }

    async fn list_metric_descriptors(
        &self,
        _request: Request<ListMetricDescriptorsRequest>,
    ) -> Result<Response<ListMetricDescriptorsResponse>, Status> {
        todo!()
    }

    async fn list_pod_sandbox_metrics(
        &self,
        _request: Request<ListPodSandboxMetricsRequest>,
    ) -> Result<Response<ListPodSandboxMetricsResponse>, Status> {
        todo!()
    }
}
