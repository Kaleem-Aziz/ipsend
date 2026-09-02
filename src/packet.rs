use std::io;
use std::mem::MaybeUninit;
use std::time::Duration;

#[derive(Default)]
pub struct Packet {
    pub id  : u64,
}

#[derive(Default)]
pub struct NetworkMetadata {
    pub sec         : libc::time_t,
    pub nsec        : libc::c_long,
    pub time        : Duration,
    pub is_hardware : bool,
}

#[derive(Default)]
pub struct NetworkPacket {
    pub packet  : Packet,            
    pub metadata: NetworkMetadata, 
}


impl Packet {
    pub fn create_packet(_id: u64) -> Packet {
        Packet { id: _id }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16]; // 16 * 8 = 128 bits
        bytes[0..8].copy_from_slice(&self.id.to_le_bytes());
        
        bytes
    }

    pub fn to_packet(_data: &[u8], _meta: &[MaybeUninit::<u8>]) -> io::Result<NetworkPacket> {
        let mut id_bytes = [0u8; 8]; // Allocate space for Bytes
        id_bytes.copy_from_slice(&_data[0..8]); // move bytes to id
        let id = u64::from_le_bytes(id_bytes); // convert bytes to int
        let packet = Packet { id };

        let metadata = Packet::process_metadata(_meta).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Required SO_TIMESTAMPING control message missing from buffer",
            )
        })?;

        Ok(NetworkPacket { packet , metadata })
    }


    pub fn process_metadata(_buf: &[MaybeUninit::<u8>]) -> Option<NetworkMetadata> {
        
        if !_buf.is_empty() {
            
            let raw_metadata_ptr = _buf.as_ptr(); 
            let metadata_len = _buf.len();

            unsafe {
                // Setup temporary msghdr using the pointer
                let mut msg: libc::msghdr = std::mem::zeroed();
                msg.msg_control = raw_metadata_ptr as *mut libc::c_void;
                msg.msg_controllen = metadata_len;

                // Hand the address of the temporary msghdr structure to the POSIX macros
                let mut cmsg = libc::CMSG_FIRSTHDR(&msg); // Point to first Contorl Message
                
                while !cmsg.is_null() {
                    // Validate the control message is from SOL_SOCKET and is SO_TIMESTAMP
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SO_TIMESTAMPING {
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
                        
                        println!("Captured TS (HW={}): {}.{:09}s", is_hardware, sec, nsec);
                        
                        let time = Duration::new(sec as u64, nsec as u32);

                        return Some(NetworkMetadata { sec, nsec, time, is_hardware });
                    
                    }
                    cmsg = libc::CMSG_NXTHDR(&msg, cmsg); // Move to next Contorl Message
                }
            }
    
        }
        
        None
    }

}