use std::io;

pub struct Packet {
    pub id: u64,
}

impl Packet {
    pub fn create_packet(id: u64) -> Packet{
        Packet { id }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16]; // 16 * 8 = 128 bits
        bytes[0..8].copy_from_slice(&self.id.to_le_bytes());
        
        bytes
    }

    pub fn to_packet(buf: &[u8]) -> io::Result<Packet> {
        let mut id_bytes = [0u8; 8]; // Allocate space for Bytes
        id_bytes.copy_from_slice(&buf[0..8]); // move bytes to id
        let id = u64::from_le_bytes(id_bytes); // convert bytes to int


        Ok(Packet{ id })
    }

}