use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WorkerRequest {
    Ping {
        id: String,
    },
    PrepareModel {
        id: String,
        model_dir: String,
        model_id: String,
    },
    Transcribe {
        id: String,
        audio_path: String,
        language: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        initial_prompt: Option<String>,
    },
}

impl WorkerRequest {
    pub fn id(&self) -> &str {
        match self {
            Self::Ping { id } | Self::PrepareModel { id, .. } | Self::Transcribe { id, .. } => id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerResponse {
    Pong {
        id: String,
        protocol_version: u32,
        engine: String,
        engine_version: String,
    },
    ModelReady {
        id: String,
        model_id: String,
        load_duration_ms: u64,
    },
    Progress {
        id: String,
        progress: f32,
    },
    Result {
        id: String,
        text: String,
        detected_language: Option<String>,
        audio_duration_ms: u64,
        processing_time_ms: u64,
    },
    Error {
        id: String,
        code: String,
        message: String,
    },
}

impl WorkerResponse {
    pub fn id(&self) -> &str {
        match self {
            Self::Pong { id, .. }
            | Self::ModelReady { id, .. }
            | Self::Progress { id, .. }
            | Self::Result { id, .. }
            | Self::Error { id, .. } => id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_serde_roundtrip() {
        let req = WorkerRequest::Transcribe {
            id: "req_1".to_string(),
            audio_path: "/tmp/sample.wav".to_string(),
            language: "en".to_string(),
            initial_prompt: Some("Acme, WidgetCo".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"action\":\"transcribe\""));

        let deserialized: WorkerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_worker_response_serde() {
        let resp = WorkerResponse::Pong {
            id: "req_0".to_string(),
            protocol_version: 2,
            engine: "mlx-whisper".to_string(),
            engine_version: "mlx-whisper-0.4.3".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"pong\""));

        let deserialized: WorkerResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }
}
