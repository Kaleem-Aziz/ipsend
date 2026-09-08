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
    pub tx_time     : Duration,
    pub packet_size : u64,   // 8 bytes

}

impl NetworkPacket {
    pub fn new(id: u64, tx_sec: u64, tx_nsec: u64, tx_time: Duration, packet_size: u64) -> Self {
        NetworkPacket { id, tx_sec, tx_nsec, tx_time, packet_size }
    }
}

#[derive(Default)]
pub struct NetworkMetadata {
    pub sec         : libc::time_t,
    pub nsec        : libc::c_long,
    pub rx_time     : Duration,
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
        

        let (id, tx_sec, tx_nsec, tx_time) = NetworkData::process_header(_hdr)?;
        let packet_size = (_data.len()) as u64 + (_hdr.len()) as u64;
        
        let packet = NetworkPacket::new(id, tx_sec, tx_nsec, tx_time, packet_size);

        let metadata = NetworkData::process_metadata(_meta).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Required SO_TIMESTAMPING control message missing from buffer",
            )
        })?;

        Ok(NetworkData { packet , metadata })
    }

    pub fn process_header(buf: &[u8]) -> io::Result<(u64, u64, u64, Duration)> {
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
        let time = Duration::new(tx_sec as u64, tx_nsec as u32);

        Ok((id, tx_sec, tx_nsec, time))
    }

    pub fn process_metadata(buf: &[u8]) -> Option<NetworkMetadata> {
        if buf.is_empty() {
            return None;
        }

        let mut ts:   Option<(i64, i64, bool)> = None;
        let mut ovfl: Option<u64>              = None;

        unsafe {
            let mut msg: libc::msghdr = std::mem::zeroed();
            msg.msg_control    = buf.as_ptr() as *mut libc::c_void;
            msg.msg_controllen = buf.len();

            let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
            while !cmsg.is_null() {
                if (*cmsg).cmsg_level == libc::SOL_SOCKET {
                    match (*cmsg).cmsg_type {

                        // Gather the timestamp of Packets
                        libc::SCM_TIMESTAMPING => {
                            let ts_ptr = libc::CMSG_DATA(cmsg) as *const libc::timespec;
                            let timestamps = std::slice::from_raw_parts(ts_ptr, 3);
                            let sw_ts = timestamps[0];
                            let hw_ts = timestamps[2];

                            let (sec, nsec, is_hardware) = if hw_ts.tv_sec != 0 {
                                (hw_ts.tv_sec, hw_ts.tv_nsec, true)
                            } else {
                                (sw_ts.tv_sec, sw_ts.tv_nsec, false)
                            };
                            ts = Some((sec, nsec, is_hardware)); 
                        }

                        // Amount of packets lost due to socket RX Queue being full
                        libc::SO_RXQ_OVFL => {
                            ovfl = Some(*(libc::CMSG_DATA(cmsg) as *const u64));
                        }
                        
                        // Default 
                        _ => {}                                  
                    }
                }
                cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
            }
        }

        let (sec, nsec, is_hardware) = ts?;
        let ovfl_count = ovfl.unwrap_or(0) as u64;

        let rx_time = Duration::new(sec as u64, nsec as u32);
        Some(NetworkMetadata { sec, nsec, rx_time, is_hardware,  ovfl_count })
    }

        
}