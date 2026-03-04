use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    AmfEncodingError, CommandMessageParseError,
    amf0::{Amf0Value, decode_amf0_values, encode_amf0_values},
};

/// Command messages carry AMF-encoded commands between the client and server.
/// Every command has a command name (String), transaction ID (Number), and
/// a command object (Object or Null), followed by optional arguments.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommandMessage {
    // ---------------------------------------------------------------
    // 7.2.1 NetConnection Commands
    // ---------------------------------------------------------------
    /// 7.2.1.1 Client -> Server
    Connect {
        transaction_id: u32,
        /// key-value pairs e.g. app, audioCodecs, videoCodecs, objectEncoding
        command_object: HashMap<String, Amf0Value>,
        optional_args: Option<Amf0Value>,
    },
    /// 7.2.1.2 Sender -> Receiver
    Call {
        procedure_name: String,
        transaction_id: u32,
        command_object: Amf0Value,
        optional_args: Option<Amf0Value>,
    },

    /// 7.2.1.3 Client -> Server
    CreateStream {
        transaction_id: u32,
        command_object: Amf0Value, // should be null
    },

    // ---------------------------------------------------------------
    // 7.2.2 NetStream Commands
    // ---------------------------------------------------------------
    /// 7.2.2.1 Client -> Server
    Play {
        transaction_id: u32,
        stream_name: String,
        start: Option<f64>,
        duration: Option<f64>,
        reset: Option<bool>,
    },

    /// 7.2.2.2 Client -> Server
    Play2 {
        transaction_id: u32,
        parameters: Amf0Value,
    },

    /// 7.2.2.3 Client -> Server
    DeleteStream { transaction_id: u32, stream_id: u32 },

    /// 7.2.2.4 Client -> Server
    ReceiveAudio {
        transaction_id: u32,
        bool_flag: bool,
    },

    /// 7.2.2.5 Client -> Server
    ReceiveVideo {
        transaction_id: u32,
        bool_flag: bool,
    },

    /// 7.2.2.6 Client -> Server
    Publish {
        stream_key: String,      // Publishing Name
        publishing_type: String, // "live" | "record" | "append"
    },

    /// 7.2.2.7 Client -> Server
    Seek {
        transaction_id: u32,
        milliseconds: f64,
    },

    /// 7.2.2.8 Client -> Server
    Pause {
        transaction_id: u32,
        pause: bool,
        milliseconds: f64,
    },

    /// Server -> Client status notification (onStatus)
    ///
    /// transaction_id = 0
    /// command_object = null
    OnStatus(Amf0Value),

    /// _result or _error response
    Result(Result<CommandMessageOk, CommandMessageError>),

    /// Close command (NetConnection)
    Close {
        transaction_id: u32,
        command_object: Amf0Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandMessageOk {
    pub transaction_id: u32,
    pub command_object: Amf0Value,
    pub response: Amf0Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandMessageError {
    transaction_id: u32,
    command_object: Amf0Value,
    response: Amf0Value,
}

pub(crate) trait CommandMessageResultExt {
    fn transaction_id(&self) -> u32;
}

impl CommandMessageResultExt for Result<CommandMessageOk, CommandMessageError> {
    fn transaction_id(&self) -> u32 {
        match self {
            Ok(val) => val.transaction_id,
            Err(err) => err.transaction_id,
        }
    }
}

impl From<CommandMessageOk> for CommandMessage {
    fn from(value: CommandMessageOk) -> Self {
        CommandMessage::Result(Ok(value))
    }
}

impl From<CommandMessageError> for CommandMessage {
    fn from(value: CommandMessageError) -> Self {
        CommandMessage::Result(Err(value))
    }
}

impl CommandMessageOk {
    /// Interpret as a connect response (spec 7.2.1.1).
    ///
    /// The connect response is `[_result, txn_id, properties_object, information_object]`.
    pub fn to_connect_success(
        &self,
    ) -> Result<CommandMessageConnectSuccess, CommandMessageParseError> {
        let properties = match &self.command_object {
            Amf0Value::Object(obj) => obj.clone(),
            Amf0Value::Null => HashMap::new(),
            _ => {
                return Err(CommandMessageParseError::UnexpectedValueType {
                    field: "properties",
                });
            }
        };

        let information = match &self.response {
            Amf0Value::Object(obj) => obj.clone(),
            Amf0Value::Null => HashMap::new(),
            _ => {
                return Err(CommandMessageParseError::UnexpectedValueType {
                    field: "information",
                });
            }
        };

        Ok(CommandMessageConnectSuccess {
            properties,
            information,
        })
    }

    /// Interpret as a createStream response (spec 7.2.1.3).
    ///
    /// The createStream response is `[_result, txn_id, null, stream_id]`.
    pub fn to_create_stream_success(
        &self,
    ) -> Result<CommandMessageCreateStreamSuccess, CommandMessageParseError> {
        let stream_id = match &self.response {
            Amf0Value::Number(n) => *n as u32,
            _ => {
                return Err(CommandMessageParseError::UnexpectedValueType { field: "stream_id" });
            }
        };
        Ok(CommandMessageCreateStreamSuccess { stream_id })
    }
}

/// Parsed connect _result response (spec 7.2.1.1, server -> client).
#[derive(Debug, Clone, PartialEq)]
#[allow(unused)]
pub(crate) struct CommandMessageConnectSuccess {
    pub properties: HashMap<String, Amf0Value>,
    pub information: HashMap<String, Amf0Value>,
}

/// Parsed createStream _result response (spec 7.2.1.3, server -> client).
#[derive(Debug, Clone, PartialEq)]
#[allow(unused)]
pub(crate) struct CommandMessageCreateStreamSuccess {
    pub stream_id: u32,
}

impl CommandMessage {
    pub fn from_amf0_bytes(payload: Bytes) -> Result<Self, CommandMessageParseError> {
        let values = decode_amf0_values(payload)?;
        Self::from_values(values)
    }

    pub fn into_amf0_bytes(self) -> Result<Bytes, AmfEncodingError> {
        let values = self.into_values();
        encode_amf0_values(&values)
    }

    fn from_values(mut values: Vec<Amf0Value>) -> Result<Self, CommandMessageParseError> {
        if values.is_empty() {
            return Err(CommandMessageParseError::MissingCommandName);
        }
        let command_name = match &values[0] {
            Amf0Value::String(s) => s.clone(),
            _ => {
                return Err(CommandMessageParseError::UnexpectedValueType {
                    field: "command_name",
                });
            }
        };

        if values.len() < 2 {
            return Err(CommandMessageParseError::MissingTransactionId);
        }
        let transaction_id = match &values[1] {
            Amf0Value::Number(n) => *n as u32,
            _ => {
                return Err(CommandMessageParseError::UnexpectedValueType {
                    field: "transaction_id",
                });
            }
        };

        match command_name.as_str() {
            "connect" => {
                let command_object = take_object(&mut values, 2)?;
                let optional_args = values.get(3).cloned();
                Ok(CommandMessage::Connect {
                    transaction_id,
                    command_object,
                    optional_args,
                })
            }

            "close" => {
                let command_object = take_value(&mut values, 2);
                Ok(CommandMessage::Close {
                    transaction_id,
                    command_object,
                })
            }

            "createStream" => {
                let command_object = take_value(&mut values, 2);
                Ok(CommandMessage::CreateStream {
                    transaction_id,
                    command_object,
                })
            }

            "_result" => {
                let command_object = take_value(&mut values, 2);
                let response = take_value(&mut values, 3);
                Ok(CommandMessage::Result(Ok(CommandMessageOk {
                    transaction_id,
                    command_object,
                    response,
                })))
            }

            "_error" => {
                let command_object = take_value(&mut values, 2);
                let response = take_value(&mut values, 3);
                Ok(CommandMessage::Result(Err(CommandMessageError {
                    transaction_id,
                    command_object,
                    response,
                })))
            }

            "onStatus" => {
                // [command_name, transaction_id=0, command_object=null, info_object]
                let info_object = take_value(&mut values, 3);
                Ok(CommandMessage::OnStatus(info_object))
            }

            "play" => {
                let stream_name = take_string(&mut values, 3)?;
                let start = take_optional_number(&values, 4);
                let duration = take_optional_number(&values, 5);
                let reset = take_optional_bool(&values, 6);
                Ok(CommandMessage::Play {
                    transaction_id,
                    stream_name,
                    start,
                    duration,
                    reset,
                })
            }

            "play2" => {
                // [command_name, transaction_id, null, parameters]
                let parameters = take_value(&mut values, 3);
                Ok(CommandMessage::Play2 {
                    transaction_id,
                    parameters,
                })
            }

            "deleteStream" => {
                let stream_id = take_number(&mut values, 3)? as u32;
                Ok(CommandMessage::DeleteStream {
                    transaction_id,
                    stream_id,
                })
            }

            "receiveAudio" => {
                let bool_flag = take_bool(&mut values, 3)?;
                Ok(CommandMessage::ReceiveAudio {
                    transaction_id,
                    bool_flag,
                })
            }

            "receiveVideo" => {
                let bool_flag = take_bool(&mut values, 3)?;
                Ok(CommandMessage::ReceiveVideo {
                    transaction_id,
                    bool_flag,
                })
            }

            "publish" => {
                let publishing_name = take_string(&mut values, 3)?;
                let publishing_type = take_string(&mut values, 4)?;
                Ok(CommandMessage::Publish {
                    stream_key: publishing_name,
                    publishing_type,
                })
            }

            "seek" => {
                let milliseconds = take_number(&mut values, 3)?;
                Ok(CommandMessage::Seek {
                    transaction_id,
                    milliseconds,
                })
            }

            "pause" => {
                let pause = take_bool(&mut values, 3)?;
                let milliseconds = take_number(&mut values, 4)?;
                Ok(CommandMessage::Pause {
                    transaction_id,
                    pause,
                    milliseconds,
                })
            }

            // Any other command name is a Call (7.2.1.2) — the procedure
            // name is used directly as the command name on the wire.
            _ => {
                let command_object = take_value(&mut values, 2);
                let optional_args = values.get(3).cloned();
                Ok(CommandMessage::Call {
                    procedure_name: command_name,
                    transaction_id,
                    command_object,
                    optional_args,
                })
            }
        }
    }

    fn into_values(self) -> Vec<Amf0Value> {
        match self {
            CommandMessage::Connect {
                transaction_id,
                command_object,
                optional_args,
            } => {
                let mut v = vec![
                    Amf0Value::String("connect".into()),
                    Amf0Value::Number(transaction_id as f64),
                    Amf0Value::Object(command_object),
                ];
                if let Some(args) = optional_args {
                    v.push(args);
                }
                v
            }

            CommandMessage::Call {
                procedure_name,
                transaction_id,
                command_object,
                optional_args,
            } => {
                let mut v = vec![
                    Amf0Value::String(procedure_name),
                    Amf0Value::Number(transaction_id as f64),
                    command_object,
                ];
                if let Some(args) = optional_args {
                    v.push(args);
                }
                v
            }

            CommandMessage::CreateStream {
                transaction_id,
                command_object,
            } => vec![
                Amf0Value::String("createStream".into()),
                Amf0Value::Number(transaction_id as f64),
                command_object,
            ],

            CommandMessage::Play {
                transaction_id,
                stream_name,
                start,
                duration,
                reset,
            } => {
                let mut v = vec![
                    Amf0Value::String("play".into()),
                    Amf0Value::Number(transaction_id as f64),
                    Amf0Value::Null,
                    Amf0Value::String(stream_name),
                ];
                if let Some(s) = start {
                    v.push(Amf0Value::Number(s));
                    if let Some(d) = duration {
                        v.push(Amf0Value::Number(d));
                        if let Some(r) = reset {
                            v.push(Amf0Value::Boolean(r));
                        }
                    }
                }
                v
            }

            CommandMessage::Play2 {
                transaction_id,
                parameters,
            } => vec![
                Amf0Value::String("play2".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                parameters,
            ],

            CommandMessage::DeleteStream {
                transaction_id,
                stream_id,
            } => vec![
                Amf0Value::String("deleteStream".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                Amf0Value::Number(stream_id as f64),
            ],

            CommandMessage::ReceiveAudio {
                transaction_id,
                bool_flag,
            } => vec![
                Amf0Value::String("receiveAudio".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                Amf0Value::Boolean(bool_flag),
            ],

            CommandMessage::ReceiveVideo {
                transaction_id,
                bool_flag,
            } => vec![
                Amf0Value::String("receiveVideo".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                Amf0Value::Boolean(bool_flag),
            ],

            CommandMessage::Publish {
                stream_key: publishing_name,
                publishing_type,
            } => vec![
                Amf0Value::String("publish".into()),
                Amf0Value::Number(0 as f64),
                Amf0Value::Null,
                Amf0Value::String(publishing_name),
                Amf0Value::String(publishing_type),
            ],

            CommandMessage::Seek {
                transaction_id,
                milliseconds,
            } => vec![
                Amf0Value::String("seek".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                Amf0Value::Number(milliseconds),
            ],

            CommandMessage::Pause {
                transaction_id,
                pause,
                milliseconds,
            } => vec![
                Amf0Value::String("pause".into()),
                Amf0Value::Number(transaction_id as f64),
                Amf0Value::Null,
                Amf0Value::Boolean(pause),
                Amf0Value::Number(milliseconds),
            ],

            CommandMessage::OnStatus(info_object) => vec![
                Amf0Value::String("onStatus".into()),
                Amf0Value::Number(0.0),
                Amf0Value::Null,
                info_object,
            ],

            CommandMessage::Result(result) => {
                let (command_name, transaction_id, command_object, response) = match result {
                    Ok(CommandMessageOk {
                        transaction_id,
                        command_object,
                        response,
                    }) => ("_result", transaction_id, command_object, response),
                    Err(CommandMessageError {
                        transaction_id,
                        command_object,
                        response,
                    }) => ("_error", transaction_id, command_object, response),
                };
                vec![
                    Amf0Value::String(command_name.into()),
                    Amf0Value::Number(transaction_id as f64),
                    command_object,
                    response,
                ]
            }

            CommandMessage::Close {
                transaction_id,
                command_object,
            } => {
                vec![
                    Amf0Value::String("close".into()),
                    Amf0Value::Number(transaction_id as f64),
                    command_object,
                ]
            }
        }
    }
}

fn take_value(values: &mut [Amf0Value], index: usize) -> Amf0Value {
    if index < values.len() {
        values[index].clone()
    } else {
        Amf0Value::Null
    }
}

fn take_object(
    values: &mut [Amf0Value],
    index: usize,
) -> Result<HashMap<String, Amf0Value>, CommandMessageParseError> {
    match values.get(index) {
        Some(Amf0Value::Object(obj)) => Ok(obj.clone()),
        Some(Amf0Value::Null) | None => Ok(HashMap::new()),
        _ => Err(CommandMessageParseError::UnexpectedValueType {
            field: "command_object",
        }),
    }
}

fn take_string(values: &mut [Amf0Value], index: usize) -> Result<String, CommandMessageParseError> {
    match values.get(index) {
        Some(Amf0Value::String(s)) => Ok(s.clone()),
        _ => Err(CommandMessageParseError::UnexpectedValueType {
            field: "string_field",
        }),
    }
}

fn take_number(values: &mut [Amf0Value], index: usize) -> Result<f64, CommandMessageParseError> {
    match values.get(index) {
        Some(Amf0Value::Number(n)) => Ok(*n),
        _ => Err(CommandMessageParseError::UnexpectedValueType {
            field: "number_field",
        }),
    }
}

fn take_bool(values: &mut [Amf0Value], index: usize) -> Result<bool, CommandMessageParseError> {
    match values.get(index) {
        Some(Amf0Value::Boolean(b)) => Ok(*b),
        _ => Err(CommandMessageParseError::UnexpectedValueType {
            field: "bool_field",
        }),
    }
}

fn take_optional_number(values: &[Amf0Value], index: usize) -> Option<f64> {
    match values.get(index) {
        Some(Amf0Value::Number(n)) => Some(*n),
        _ => None,
    }
}

fn take_optional_bool(values: &[Amf0Value], index: usize) -> Option<bool> {
    match values.get(index) {
        Some(Amf0Value::Boolean(b)) => Some(*b),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_roundtrip() {
        let mut obj = HashMap::new();
        obj.insert("app".into(), Amf0Value::String("testapp".into()));
        obj.insert(
            "tcUrl".into(),
            Amf0Value::String("rtmp://localhost/testapp".into()),
        );

        let msg = CommandMessage::Connect {
            transaction_id: 1,
            command_object: obj.clone(),
            optional_args: None,
        };

        let encoded = msg.clone().into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::Connect {
                transaction_id,
                command_object,
                optional_args,
            } => {
                assert_eq!(transaction_id, 1);
                assert_eq!(command_object, obj);
                assert!(optional_args.is_none());
            }
            other => panic!("Expected Connect, got {other:?}"),
        }
    }

    #[test]
    fn create_stream_roundtrip() {
        let msg = CommandMessage::CreateStream {
            transaction_id: 2,
            command_object: Amf0Value::Null,
        };

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::CreateStream {
                transaction_id,
                command_object,
            } => {
                assert_eq!(transaction_id, 2);
                assert_eq!(command_object, Amf0Value::Null);
            }
            other => panic!("Expected CreateStream, got {other:?}"),
        }
    }

    #[test]
    fn publish_roundtrip() {
        let msg = CommandMessage::Publish {
            stream_key: "mystream".into(),
            publishing_type: "live".into(),
        };

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::Publish {
                stream_key: publishing_name,
                publishing_type,
            } => {
                assert_eq!(publishing_name, "mystream");
                assert_eq!(publishing_type, "live");
            }
            other => panic!("Expected Publish, got {other:?}"),
        }
    }

    #[test]
    fn play_roundtrip() {
        let msg = CommandMessage::Play {
            transaction_id: 0,
            stream_name: "test".into(),
            start: Some(-2.0),
            duration: Some(-1.0),
            reset: Some(true),
        };

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::Play {
                transaction_id,
                stream_name,
                start,
                duration,
                reset,
            } => {
                assert_eq!(transaction_id, 0);
                assert_eq!(stream_name, "test");
                assert_eq!(start, Some(-2.0));
                assert_eq!(duration, Some(-1.0));
                assert_eq!(reset, Some(true));
            }
            other => panic!("Expected Play, got {other:?}"),
        }
    }

    #[test]
    fn on_status_roundtrip() {
        let mut info = HashMap::new();
        info.insert("level".into(), Amf0Value::String("status".into()));
        info.insert(
            "code".into(),
            Amf0Value::String("NetStream.Play.Start".into()),
        );
        info.insert(
            "description".into(),
            Amf0Value::String("Started playing.".into()),
        );

        let msg = CommandMessage::OnStatus(Amf0Value::Object(info.clone()));

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::OnStatus(info_object) => {
                assert_eq!(info_object, Amf0Value::Object(info));
            }
            other => panic!("Expected OnStatus, got {other:?}"),
        }
    }

    #[test]
    fn result_roundtrip() {
        let msg = CommandMessage::Result(Ok(CommandMessageOk {
            transaction_id: 1,
            command_object: Amf0Value::Null,
            response: Amf0Value::Number(1.0),
        }));

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::Result(Ok(CommandMessageOk {
                transaction_id,
                command_object,
                response,
            })) => {
                assert_eq!(transaction_id, 1);
                assert_eq!(command_object, Amf0Value::Null);
                assert_eq!(response, Amf0Value::Number(1.0));
            }
            other => panic!("Expected Result(Ok), got {other:?}"),
        }
    }

    #[test]
    fn delete_stream_roundtrip() {
        let msg = CommandMessage::DeleteStream {
            transaction_id: 0,
            stream_id: 1,
        };

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::DeleteStream {
                transaction_id,
                stream_id,
            } => {
                assert_eq!(transaction_id, 0);
                assert_eq!(stream_id, 1);
            }
            other => panic!("Expected DeleteStream, got {other:?}"),
        }
    }

    #[test]
    fn pause_roundtrip() {
        let msg = CommandMessage::Pause {
            transaction_id: 0,
            pause: true,
            milliseconds: 5000.0,
        };

        let encoded = msg.into_amf0_bytes().unwrap();
        let decoded = CommandMessage::from_amf0_bytes(encoded).unwrap();

        match decoded {
            CommandMessage::Pause {
                transaction_id,
                pause,
                milliseconds,
            } => {
                assert_eq!(transaction_id, 0);
                assert!(pause);
                assert_eq!(milliseconds, 5000.0);
            }
            other => panic!("Expected Pause, got {other:?}"),
        }
    }
}
