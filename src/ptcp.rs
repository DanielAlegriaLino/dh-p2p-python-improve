use async_trait::async_trait;
use std::cmp;
use tokio::net::UdpSocket;

pub enum PTCPEvent {
    Connect(u32),
    Disconnect(u32),
    Data(u32, Vec<u8>),
}

pub struct PTCPPayload {
    pub realm: u32,
    pub data: Vec<u8>,
}

pub enum PTCPBody {
    Command(Vec<u8>),
    Payload(PTCPPayload),
    Empty,
}

pub struct PTCPPacket {
    sent: u32,
    recv: u32,
    pid: u32,
    lmid: u32,
    rmid: u32,
    pub body: PTCPBody,
}

impl PTCPPayload {
    fn parse(data: &[u8]) -> PTCPPayload {
        assert!(data.len() >= 12, "Invalid payload");
        assert_eq!(data[0], 0x10, "Invalid header");

        // first 4 bytes it header
        let header = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let length = header & 0xFFFF;
        let realm = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let padding = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let data = data[12..].to_vec();

        assert_eq!(padding, 0, "Invalid padding");
        assert_eq!(length, data.len() as u32, "Invalid length");

        PTCPPayload { realm, data }
    }

    fn serialize(&self) -> Vec<u8> {
        let length = self.data.len() as u32;
        let header = 0x10000000 | length;
        let header = header.to_be_bytes();
        let realm = self.realm.to_be_bytes();
        let padding = 0u32.to_be_bytes();

        let mut buf = Vec::new();
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&realm);
        buf.extend_from_slice(&padding);
        buf.extend_from_slice(&self.data);

        buf
    }
}

impl std::fmt::Debug for PTCPPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "length: {}, realm: 0x{:08x}, data: [{}{}]",
            self.data.len(),
            self.realm,
            self.data[0..cmp::min(self.data.len(), 16)]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" "),
            if self.data.len() > 16 { " ..." } else { "" },
        )
    }
}

impl std::fmt::Debug for PTCPBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PTCPBody::Command(data) => write!(
                f,
                "Command([{}])",
                data.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            PTCPBody::Payload(payload) => write!(f, "{:?}", payload),
            PTCPBody::Empty => write!(f, "Empty"),
        }
    }
}

impl std::fmt::Debug for PTCPPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PTCPPacket {{ sent: {}, recv: {}, pid: 0x{:08x}, lmid: 0x{:08x}, rmid: 0x{:08x}, body: {:?} }}",
            self.sent, self.recv, self.pid, self.lmid, self.rmid, self.body
        )
    }
}

impl PTCPBody {
    fn parse(data: &[u8]) -> PTCPBody {
        if data.len() == 0 {
            return PTCPBody::Empty;
        }

        assert!(data.len() >= 4, "Invalid body");

        if data[0] == 0x10 {
            PTCPBody::Payload(PTCPPayload::parse(data))
        } else {
            PTCPBody::Command(data.to_vec())
        }
    }

    fn serialize(&self) -> Vec<u8> {
        match self {
            PTCPBody::Command(data) => data.to_vec(),
            PTCPBody::Payload(payload) => payload.serialize(),
            PTCPBody::Empty => Vec::new(),
        }
    }
}

impl PTCPPacket {
    fn parse(data: &[u8]) -> PTCPPacket {
        assert!(data.len() >= 24, "Invalid packet");

        let magic = &data[0..4];

        assert_eq!(magic, b"PTCP", "Invalid magic");

        let sent = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let recv = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let pid = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let lmid = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let rmid = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        let body = PTCPBody::parse(&data[24..]);

        PTCPPacket {
            sent,
            recv,
            pid,
            lmid,
            rmid,
            body,
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PTCP");
        buf.extend_from_slice(&self.sent.to_be_bytes());
        buf.extend_from_slice(&self.recv.to_be_bytes());
        buf.extend_from_slice(&self.pid.to_be_bytes());
        buf.extend_from_slice(&self.lmid.to_be_bytes());
        buf.extend_from_slice(&self.rmid.to_be_bytes());
        buf.extend_from_slice(&self.body.serialize());

        buf
    }
}

pub struct PTCPSession {
    sent: u32,
    recv: u32,
    count: u32,
    id: u32,
    rmid: u32,
}

impl PTCPSession {
    pub fn new() -> PTCPSession {
        PTCPSession {
            sent: 0,
            recv: 0,
            count: 0,
            id: 0,
            rmid: 0,
        }
    }

    pub fn send(&mut self, body: PTCPBody) -> PTCPPacket {
        let is_ack = match body {
            PTCPBody::Command(ref c) => c == b"\x00\x03\x01\x00",
            _ => false,
        };

        let sent = self.sent;
        let recv = self.recv;
        let pid = if is_ack {
            0x0002FFFF
        } else {
            0x0000FFFF - self.count
        };
        let lmid = self.id;
        let rmid = self.rmid;

        /*
         * Update counters
         */
        self.sent += match body {
            PTCPBody::Command(ref c) => c.len() as u32,
            PTCPBody::Payload(ref p) => p.data.len() as u32 + 12,
            PTCPBody::Empty => 0,
        };

        self.id += 1;
        self.count += match body {
            PTCPBody::Command(_) => {
                if is_ack {
                    0
                } else {
                    1
                }
            }
            PTCPBody::Payload(_) => 1,
            PTCPBody::Empty => 0,
        };

        PTCPPacket {
            sent,
            recv,
            pid,
            lmid,
            rmid,
            body,
        }
    }

    pub fn recv(&mut self, packet: PTCPPacket) -> PTCPPacket {
        self.recv += match packet.body {
            PTCPBody::Command(ref c) => c.len() as u32,
            PTCPBody::Payload(ref p) => p.data.len() as u32 + 12,
            PTCPBody::Empty => 0,
        };
        self.rmid = packet.lmid;

        packet
    }
}

#[async_trait]
pub trait PTCP {
    async fn ptcp_request(&self, packet: PTCPPacket);
    async fn ptcp_read(&self) -> PTCPPacket;
}

#[async_trait]
impl PTCP for UdpSocket {
    async fn ptcp_request(&self, packet: PTCPPacket) {
        println!(">>> {}", self.peer_addr().unwrap());
        println!("{:?}", packet);

        let packet = packet.serialize();
        self.send(&packet).await.unwrap();
        println!("---");
    }

    async fn ptcp_read(&self) -> PTCPPacket {
        println!("<<< {}", self.peer_addr().unwrap());

        let mut buf = [0u8; 4096];
        let n = self.recv(&mut buf).await.unwrap();
        let packet = PTCPPacket::parse(&buf[0..n]);
        println!("{:?}", packet);
        println!("---");

        packet
    }
}