use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};


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


#[derive(Default)]
pub struct NetworkPacket {
    pub id          : u64,   // 8 bytes
    pub tx_sec      : u64,   // 8 bytes 
    pub tx_nsec     : u64,   // 8 bytes 
    pub packet_size : u64,   // 8 bytes

}

impl NetworkPacket {
    pub fn new(id: u64, tx_sec: u64, tx_nsec: u64, packet_size: u64) -> Self {
        NetworkPacket { id, tx_sec, tx_nsec, packet_size }
    }
}

#[derive(Default)]
pub struct NetworkMetadata {
    pub sec         : libc::time_t,
    pub nsec        : libc::c_long,
    pub time        : Duration,
    pub is_hardware : bool,
    pub ovfl_count  : u64,
}

#[derive(Default)]
pub struct NetworkData {
    pub packet  : NetworkPacket,            
    pub metadata: NetworkMetadata, 
}


impl NetworkData {

    pub fn to_packet(_hdr: &[u8], _data: &[u8], _meta: &[u8]) -> io::Result<NetworkData> {
        

        let (id, tx_sec, tx_nsec) = NetworkData::process_header(_hdr)?;
        let packet_size = (_data.len()) as u64;
        
        let packet = NetworkPacket::new(id, tx_sec, tx_nsec, packet_size);

        let metadata = NetworkData::process_metadata(_meta).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Required SO_TIMESTAMPING control message missing from buffer",
            )
        })?;

        Ok(NetworkData { packet , metadata })
    }

    pub fn process_header(buf: &[u8]) -> io::Result<(u64, u64, u64)> {
        // Ensure the buffer has at least 24 bytes to prevent panics
        if buf.len() < 24 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Buffer too short to parse header",
            ));
        }

        // Convert slices 
        let id = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let tx_sec = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let tx_nsec = u64::from_le_bytes(buf[16..24].try_into().unwrap());

        Ok((id, tx_sec, tx_nsec))
    }

    pub fn process_metadata(_buf: &[u8]) -> Option<NetworkMetadata> {
        
        if !_buf.is_empty() { 
            unsafe {
                // Setup temporary msghdr using the pointer
                let mut msg: libc::msghdr = std::mem::zeroed();
                msg.msg_control    = _buf.as_ptr() as *mut libc::c_void;   
                msg.msg_controllen = _buf.len();

                
                let mut ts: Option<(i64, i64, bool)> = None;
                let mut ovfl: Option<u32> = None;
                // Hand the address of the temporary msghdr structure to the POSIX macros
                let mut cmsg = libc::CMSG_FIRSTHDR(&msg); // Point to first Contorl Message
                
                while !cmsg.is_null() {
                    // Validate the control message is from SOL_SOCKET and is SCM_TIMESTAMPING
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET {
                        
                        match (*cmsg).cmsg_type {
                            libc::SCM_TIMESTAMPING => {
                                let ts_ptr = libc::CMSG_DATA(cmsg) as *const libc::timespec;
                                // Grab and split timestamps
                                let timestamps = std::slice::from_raw_parts(ts_ptr, 3);
                                
                                let sw_ts = timestamps[0]; // Filled on loopback and physical networks
                                let hw_ts = timestamps[2]; // Only filled on real NICs with HW support

                                let (sec, nsec, is_hardware) = if hw_ts.tv_sec != 0 {
                                    (hw_ts.tv_sec, hw_ts.tv_nsec, true)
                                } else {
                                    (sw_ts.tv_sec, sw_ts.tv_nsec, false)
                                };
                                                        
                                let time = Duration::new(sec as u64, nsec as u32);
                                ts = Some(sec, nsec, is_hardware);
                            }

                            libc::SO_RXQ_OVFL => {
                                ovfl = Some(*(libc::CMSG_DATA(cmsg) as *const u32));
                            }

                        }
                            
                    }
                    cmsg = libc::CMSG_NXTHDR(&msg, cmsg); // Move to next Contorl Message
                }
            }
        }
        Some(NetworkMetadata { sec, nsec, time, is_hardware, ovfl })
    }
        
}