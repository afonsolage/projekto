use projekto_core::{chunk::Chunk, voxel};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("Failed to deserialize message. Error: {0}")]
    Serde(#[from] bincode::Error),
}

pub struct MessageBuffer(Vec<u8>);

impl MessageBuffer {
    pub fn new(buffer: Vec<u8>) -> Self {
        Self(buffer)
    }

    pub fn try_from_req(req: &MessageReq) -> Result<Self, MessageError> {
        Ok(MessageBuffer(bincode::serialize(req)?))
    }

    pub fn try_from_res(req: &MessageRes) -> Result<Self, MessageError> {
        Ok(MessageBuffer(bincode::serialize(req)?))
    }

    pub fn deserialize_req(&self) -> Result<MessageReq, MessageError> {
        Ok(bincode::deserialize(&self.0)?)
    }

    pub fn deserialize_res(&self) -> Result<MessageRes, MessageError> {
        Ok(bincode::deserialize(&self.0)?)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageReq {
    ChunkLoadReq { chunk: Chunk },
    End,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageRes {
    ChunkLoadRes {
        chunk: Chunk,
        vertex: Vec<voxel::Vertex>,
    },
    End,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_req() {
        let req = MessageReq::ChunkLoadReq {
            chunk: Chunk::new(-8, 1234),
        };

        let msg = MessageBuffer::try_from_req(&req).unwrap();
        assert!(!msg.is_empty());
    }

    #[test]
    fn serialize_res() {
        let res = MessageRes::ChunkLoadRes {
            chunk: Chunk::new(-8, 1234),
            vertex: vec![voxel::Vertex::default()],
        };

        let msg = MessageBuffer::try_from_res(&res).unwrap();
        assert!(!msg.is_empty());
    }

    #[test]
    fn deserialize_req() {
        let test_chunk = Chunk::new(-8, 1234);
        let req = MessageReq::ChunkLoadReq { chunk: test_chunk };

        let msg = MessageBuffer::try_from_req(&req).unwrap();
        let des_req = msg.deserialize_req().unwrap();

        match des_req {
            MessageReq::ChunkLoadReq { chunk } => assert_eq!(chunk, test_chunk),
            _ => panic!(),
        }
    }

    #[test]
    fn deserialize_res() {
        let test_chunk = Chunk::new(-8, 1234);
        let test_vertex = vec![voxel::Vertex::default()];
        let res = MessageRes::ChunkLoadRes {
            chunk: test_chunk,
            vertex: test_vertex.clone(),
        };

        let msg = MessageBuffer::try_from_res(&res).unwrap();
        let des_res = msg.deserialize_res().unwrap();

        match des_res {
            MessageRes::ChunkLoadRes { chunk, vertex } => {
                assert_eq!(chunk, test_chunk);
                assert_eq!(vertex, test_vertex);
            }
            _ => panic!(),
        }
    }
}
