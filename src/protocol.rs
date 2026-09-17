use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};


pub struct NetworkTimestamp {
    pub time: Duration,
}

impl Default for NetworkTimestamp {
    fn default() -> Self {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO); 
        Self { time }
    }
}

impl NetworkTimestamp {
    pub fn new(sec: u64, nsec: u32) -> Self {
        Self { time: Duration::new(sec, nsec) }
    }

    pub fn sec(&self)  -> u64 { self.time.as_secs() }
    pub fn nsec(&self) -> u32 { self.time.subsec_nanos() }
}

impl From<&libc::timespec> for NetworkTimestamp {
    fn from(ts: &libc::timespec) -> Self {
        Self {
            time: Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32),
        }
    }
}


pub struct Header {
    pub id          : u64,   
    pub timestamp   : NetworkTimestamp
}

impl Header {

    pub const HEADER_SIZE  :usize = 20;

    pub fn new(id: u64) -> Self {
        Self { id,  timestamp: NetworkTimestamp::default()}
    }

    pub fn encode(&self, buf: &mut [u8]) -> io::Result<()> {
        if buf.len() < Self::HEADER_SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
        }

        buf[0..8].copy_from_slice(&self.id.to_le_bytes());
        buf[8..16].copy_from_slice(&self.timestamp.sec().to_le_bytes());
        buf[16..20].copy_from_slice(&self.timestamp.nsec().to_le_bytes());

        Ok(())
    }   

    pub fn decode(buf: &[u8]) -> io::Result<(Self)> {
        // Ensure the buffer has at least 24 bytes to prevent panics
        if buf.len() < Self::HEADER_SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
        }

        // Convert slices 
        let id = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let tx_sec = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let tx_nsec = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        
        let timestamp = NetworkTimestamp::new(tx_sec, tx_nsec );

        Ok(Self { id, timestamp })
    }

}



pub struct Packet<'a> {
    pub header      : Header,
    pub payload     : &'a mut [u8],  // x bytes
}

impl <'a> Packet<'a>  {
    pub fn new(id: u64, payload: &'a mut [u8]) -> Self {
        Packet { header: Header::new(id) , payload  }
    }

    pub fn to_bytes(&mut self) -> &[u8] {
        self.header.encode(self.payload); 

        self.payload
    }
}
