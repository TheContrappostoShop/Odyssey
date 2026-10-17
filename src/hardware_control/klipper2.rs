use std::{collections::HashMap, fmt::Display, sync::Arc, time::Duration};

use poem_openapi::Enum;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{io, net::UnixStream, sync::{RwLock, mpsc::{self, error::TryRecvError::{Disconnected, Empty}}, oneshot}, time::timeout};
use tokio_stream::wrappers::WatchStream;
use tracing::instrument;




#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KlipperResponseObject {
    pub id: Option<String>,
    pub source: Option<String>,
    pub result: Option<Value>,
    pub params: Option<Value>,
    #[serde(rename = "eventtime")]
    pub event_time: Option<f32>,
}
impl Display for KlipperResponseObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum KlipperRequestMethod {
    #[serde(rename = "emergency_stop")]
    EmergencyStop,

    #[serde(rename = "objects/list")]
    ObjectsList,
    #[serde(rename = "objects/query")]
    ObjectsQuery,
    #[serde(rename = "objects/subscribe")]
    ObjectsSubscribe,

    #[serde(rename = "gcode/script")]
    GcodeScript,
    #[serde(rename = "gcode/restart")]
    GcodeRestart,
    #[serde(rename = "gcode/firmware_restart")]
    GcodeFirmwareRestart,
    #[serde(rename = "gcode/subscribe_output")]
    GcodeSubscribeOutput,
}

pub struct KlipperRequestWithCallback(KlipperRequest,Option<oneshot::Sender<Value>>);


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KlipperRequest {
    pub id: String,
    pub method: KlipperRequestMethod,
    pub params: Option<Value>,
}
impl KlipperRequest {
    fn emergency_stop(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::EmergencyStop,
            params: None,
        }
    }

    fn objects_list(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsList,
            params: None,
        }
    }

    fn objects_query(id: &str, objects: HashMap<String, Option<Vec<String>>>) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsQuery,
            params: Some(json!({
                "objects": objects
            })),
        }
    }

    fn objects_subscribe(
        id: &str,
        objects: &HashMap<&str, Option<Vec<&str>>>,
        source: &str,
    ) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsSubscribe,
            params: Some(json!({
                "objects": objects,
                "response_template": {
                    "source": source
                }
            })),
        }
    }

    fn gcode_script(id: &str, script: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeScript,
            params: Some(json!({
                "script": script
            })),
        }
    }

    fn gcode_restart(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeRestart,
            params: None,
        }
    }

    fn gcode_firmware_restart(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeFirmwareRestart,
            params: None,
        }
    }

    fn gcode_subscribe_output(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeSubscribeOutput,
            params: Some(json!({
                "response_template": {
                    "source": "gcode"
                }
            })),
        }
    }
}

pub struct KlipperUDSController {
    klipper_uds: UnixStream,
    variables: Arc<RwLock<HashMap<String, WatchStream<Value>>>>,
    request_sender: mpsc::Sender<KlipperRequestWithCallback>,
    request_receiver: mpsc::Receiver<KlipperRequestWithCallback>,
}

impl KlipperUDSController {
    fn get_sender(&self) -> mpsc::Sender<KlipperRequestWithCallback> {
        self.request_sender.clone()
    }
    fn uds_reconnect(&self) -> () {
    }

}


pub struct KlipperUDSMonitor {
    klipper_uds: UnixStream,
    variables: Arc<RwLock<HashMap<String, WatchStream<Value>>>>,
    request_receiver: mpsc::Receiver<KlipperRequestWithCallback>,
    response_send_channels: Arc<RwLock<HashMap<String, oneshot::Sender<Value>>>>,

}

impl KlipperUDSMonitor {
    pub async fn send_request_with_callback(
        &self,
        request: &KlipperRequest,
        callback: oneshot::Sender<Value>) -> Result<(), io::Error>  {
        self.response_send_channels.write().await.insert(request.id.clone(), callback);

        self.send_request(&request)
    }
    
    #[instrument(skip_all, level = "debug", fields(id = request.id), ret)]
    pub async fn send_request(&self, request: &KlipperRequest) -> Result<(), io::Error> {
        let mut request_bytes = serde_json::to_vec(&request)?;
        request_bytes.push(0x03);
        timeout(Duration::from_secs(10), async {
            loop {
                self.klipper_uds.writable().await?;

                match self.klipper_uds.try_write(&request_bytes) {
                    Ok(n) => {
                        tracing::debug!("wrote {} bytes to Klipper:\n{:#?}", n, request);
                        return Ok(());
                    }
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        _ => {
                            tracing::error!("Error encountered sending request to Klipper:\n{}", e);
                            return Err(e);
                        }
                    },
                }
            }
        })
        .await
        .map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))
        .flatten()
    }


    async fn read_request_channel(&self) {
        match self.request_receiver.try_recv()  {
            Ok(KlipperRequestWithCallback(request, Some(callback))) => self.send_request_with_callback(&request, callback),
            Ok(KlipperRequestWithCallback(request, None)) => self.send_request(&request),
            Err(Empty) => todo!(),
            Err(Disconnected) => todo!(),
        }.await;
    }
}