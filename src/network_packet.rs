// pub struct NetworkPacket {
//     pub id          : u64,       // 8 bytes
//     pub tx_sec      : Instant,   // 16 bytes
// }

// impl Default for NetworkPacket {
//     fn default() -> Self {
//         NetworkPacket {
//             id: 0,
//             tx_sec: Instant::now(),
//         }
//     }
// }

// impl NetworkPacket {
//     pub fn new(id: u64, tx_sec: Instant) -> Self {
//         NetworkPacket { id, tx_sec }
//     }
// }