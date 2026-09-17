use std::time::{SystemTime, UNIX_EPOCH};


pub struct Packet<'a> {
    pub id          : u64,       // 8 bytes
    pub tx_sec      : u64,   // 8 bytes 
    pub tx_nsec     : u64,  // 8 bytes 
    pub payload     : &'a mut [u8],  // x bytes
}

impl <'a> Packet<'a>  {
    pub fn new(id: u64, payload: &'a mut [u8]) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Packet { id, tx_sec: now.as_secs(), tx_nsec: now.subsec_nanos() as u64,  payload  }
    }

    pub fn to_bytes(&mut self) -> &[u8] {
        
        self.payload[0..8].copy_from_slice(&self.id.to_le_bytes());
        self.payload[8..16].copy_from_slice(&self.tx_sec.to_le_bytes());
        self.payload[16..24].copy_from_slice(&self.tx_nsec.to_le_bytes());

        self.payload
    }    

}
