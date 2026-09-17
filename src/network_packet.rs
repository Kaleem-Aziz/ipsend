use std::io;
use std::time::{Duration};






#[derive(Default, Clone, Copy) ]
pub struct NetworkPacket {
    pub id              : u64,   // 8 bytes
    pub tx_timestamp    : NetworkTimestamp,
    pub packet_size     : u64,   // 8 bytes

}

impl NetworkPacket {
    pub fn new(id: u64, tx_timestamp: NetworkTimestamp, packet_size: u64) -> Self {
        NetworkPacket { id, tx_timestamp, packet_size }
    }
}

#[derive(Default, Clone, Copy)]
pub struct NetworkMetadata {
    pub sw_timestamp    : NetworkTimestamp,
    pub hw_timestamp    : Option<NetworkTimestamp>,
    pub ovfl_count      : u32,
}

#[derive(Default, Clone, Copy)]
pub struct NetworkTimestamp {
    pub time: Duration,
}

impl NetworkTimestamp {
    pub fn new(sec: u64, nsec: u32) -> Self {
        Self { time: Duration::new(sec, nsec) }
    }

    pub fn from_timespec(ts: &libc::timespec) -> Self {
        Self { time: Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32) }
    }
    // pub fn sec(&self)  -> u64 { self.time.as_secs() }
    // pub fn nsec(&self) -> u32 { self.time.subsec_nanos() }
}


#[derive(Default, Clone, Copy)]
pub struct NetworkData {
    pub packet  : NetworkPacket,            
    pub metadata: NetworkMetadata, 
}


impl NetworkData {

    pub fn to_packet(_hdr: &[u8], _data: &[u8], _meta: &[u8]) -> io::Result<NetworkData> {
        

        let (id, tx_timestamp) = NetworkData::process_header(_hdr)?;
        let packet_size = (_data.len()) as u64 + (_hdr.len()) as u64;
        
        let packet = NetworkPacket::new(id, tx_timestamp, packet_size);

        let metadata = NetworkData::process_metadata(_meta).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Required SO_TIMESTAMPING control message missing from buffer",
            )
        })?;

        Ok(NetworkData { packet , metadata })
    }

    pub fn process_header(buf: &[u8]) -> io::Result<(u64, NetworkTimestamp)> {
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
        
        let tx_timestamp = NetworkTimestamp::new(tx_sec, tx_nsec as u32);

        Ok((id, tx_timestamp))
    }

    pub fn process_metadata(buf: &[u8]) -> Option<NetworkMetadata> {
        if buf.is_empty() {
            return None;
        }

        let mut sw_timestamp: Option<NetworkTimestamp> = None;
        let mut hw_timestamp: Option<NetworkTimestamp> = None;
        let mut ovfl: Option<u32>   = None;

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
                            
                            if sw_ts.tv_sec != 0 || sw_ts.tv_nsec != 0 {
                                sw_timestamp = Some(NetworkTimestamp::from_timespec(&sw_ts));
                            }

                            let hw_ts = timestamps[2];
                            if hw_ts.tv_sec != 0 || hw_ts.tv_nsec != 0 {
                                hw_timestamp = Some(NetworkTimestamp::from_timespec(&hw_ts));
                            }

                        }

                        // Amount of packets lost due to socket RX Queue being full
                        libc::SO_RXQ_OVFL => {
                            ovfl = Some(*(libc::CMSG_DATA(cmsg) as *const u32));
                        }
                        
                        // Default 
                        _ => {}                                  
                    }
                }
                cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
            }
        }

        let ovfl_count = ovfl.unwrap_or(0);

        Some(NetworkMetadata { sw_timestamp: sw_timestamp?, hw_timestamp,  ovfl_count })
    }

        
}