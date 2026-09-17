use std::io;
use std::time::{Duration};

use crate::protocol::{NetworkTimestamp, Header}; 


pub struct NetworkData {
    pub header      : Header,            
    pub metadata    : Metadata, 
    pub packet_size : u64,  
}


impl NetworkData {

    pub fn to_packet(_hdr: &[u8], _data: &[u8], _meta: &[u8]) -> io::Result<Self> {
        
        let packet_size = (_data.len()) as u64 + (_hdr.len()) as u64;
        
        let header = Header::decode(_hdr);

        let metadata = Metadata::process(_meta).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Required SO_TIMESTAMPING control message missing from buffer",
            )
        })?;

        Ok(Self { header: header? , metadata, packet_size })
    }

       
}

pub struct Metadata {
    pub sw_timestamp    : NetworkTimestamp,
    pub hw_timestamp    : Option<NetworkTimestamp>,
    pub ovfl_count      : u32, 

}

impl Metadata {

    pub fn process(buf: &[u8]) -> Option<Self> {
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
                                sw_timestamp = Some(NetworkTimestamp::from(&sw_ts));
                            }

                            let hw_ts = timestamps[2];
                            if hw_ts.tv_sec != 0 || hw_ts.tv_nsec != 0 {
                                hw_timestamp = Some(NetworkTimestamp::from(&hw_ts));
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

        Some(Self { sw_timestamp: sw_timestamp?, hw_timestamp,  ovfl_count })
    }
}