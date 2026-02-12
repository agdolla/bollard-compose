/// Events emitted during compose operations.
#[derive(Debug, Clone)]
pub enum ComposeEvent {
    PullingImage {
        image: String,
    },
    ImagePulled {
        image: String,
    },
    CreatingNetwork {
        network: String,
    },
    NetworkCreated {
        network: String,
    },
    CreatingContainer {
        service: String,
        container: String,
    },
    ContainerCreated {
        service: String,
        container: String,
    },
    StartingContainer {
        service: String,
        container: String,
    },
    ContainerStarted {
        service: String,
        container: String,
    },
    StoppingContainer {
        service: String,
        container: String,
    },
    ContainerStopped {
        service: String,
        container: String,
    },
    RemovingContainer {
        service: String,
        container: String,
    },
    ContainerRemoved {
        service: String,
        container: String,
    },
    RemovingNetwork {
        network: String,
    },
    NetworkRemoved {
        network: String,
    },
    ServiceReady {
        service: String,
    },
    Error {
        service: Option<String>,
        message: String,
    },
}
