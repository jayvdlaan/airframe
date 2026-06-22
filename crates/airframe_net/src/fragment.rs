use crate::NetError;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Header size for a fragment: u16 message_id + u8 index + u8 count = 4 bytes.
const FRAGMENT_HEADER_SIZE: usize = 4;

/// Default cap on concurrently-pending (incomplete) reassemblies. Bounds memory
/// against an attacker who streams fragments for many distinct message ids and
/// never completes them (memory-amplification DoS).
const DEFAULT_MAX_PENDING_ASSEMBLIES: usize = 1024;

/// Header for a fragment.
#[derive(Debug, Clone)]
pub struct FragmentHeader {
    pub message_id: u16,
    pub fragment_index: u8,
    pub fragment_count: u8,
}

impl FragmentHeader {
    fn encode(&self) -> [u8; FRAGMENT_HEADER_SIZE] {
        let id = self.message_id.to_be_bytes();
        [id[0], id[1], self.fragment_index, self.fragment_count]
    }

    fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < FRAGMENT_HEADER_SIZE {
            return None;
        }
        Some(Self {
            message_id: u16::from_be_bytes([data[0], data[1]]),
            fragment_index: data[2],
            fragment_count: data[3],
        })
    }
}

struct PendingAssembly {
    fragments: Vec<Option<Vec<u8>>>,
    total: u8,
    received: u8,
    started: Instant,
}

pub struct FragmentAssembler {
    pending: HashMap<u16, PendingAssembly>,
    next_message_id: u16,
    timeout: Duration,
    max_pending: usize,
}

impl FragmentAssembler {
    pub fn new(timeout: Duration) -> Self {
        Self::with_max_pending(timeout, DEFAULT_MAX_PENDING_ASSEMBLIES)
    }

    /// Construct with an explicit cap on concurrently-pending reassemblies.
    pub fn with_max_pending(timeout: Duration, max_pending: usize) -> Self {
        Self {
            pending: HashMap::new(),
            next_message_id: 0,
            timeout,
            max_pending: max_pending.max(1),
        }
    }

    /// Split a large message into fragments. Returns a Vec of fragment payloads
    /// each prefixed with FragmentHeader.
    pub fn fragment(&mut self, data: &[u8], max_fragment_size: usize) -> Vec<Vec<u8>> {
        assert!(
            max_fragment_size > FRAGMENT_HEADER_SIZE,
            "max_fragment_size must exceed header size"
        );

        let payload_capacity = max_fragment_size - FRAGMENT_HEADER_SIZE;
        let fragment_count = if data.is_empty() {
            1
        } else {
            data.chunks(payload_capacity).len()
        };

        let message_id = self.next_message_id;
        self.next_message_id = self.next_message_id.wrapping_add(1);

        let mut fragments = Vec::with_capacity(fragment_count);

        if data.is_empty() {
            let header = FragmentHeader {
                message_id,
                fragment_index: 0,
                fragment_count: 1,
            };
            let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE);
            buf.extend_from_slice(&header.encode());
            fragments.push(buf);
        } else {
            for (i, chunk) in data.chunks(payload_capacity).enumerate() {
                let header = FragmentHeader {
                    message_id,
                    fragment_index: i as u8,
                    fragment_count: fragment_count as u8,
                };
                let mut buf = Vec::with_capacity(FRAGMENT_HEADER_SIZE + chunk.len());
                buf.extend_from_slice(&header.encode());
                buf.extend_from_slice(chunk);
                fragments.push(buf);
            }
        }

        fragments
    }

    /// Process an incoming fragment. Returns the reassembled message if all
    /// fragments have been received, or None if still waiting.
    pub fn receive_fragment(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>, NetError> {
        let header = FragmentHeader::decode(data).ok_or_else(|| {
            NetError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "fragment too short for header",
            ))
        })?;

        if header.fragment_count == 0 || header.fragment_index >= header.fragment_count {
            return Err(NetError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid fragment header",
            )));
        }

        let payload = data[FRAGMENT_HEADER_SIZE..].to_vec();

        // Bound memory: before starting a NEW reassembly, enforce the pending cap.
        // First reclaim timed-out assemblies; if still at the limit, evict the
        // oldest so a flood of distinct message ids cannot grow `pending` forever.
        if !self.pending.contains_key(&header.message_id) && self.pending.len() >= self.max_pending
        {
            self.cleanup_stale();
            if self.pending.len() >= self.max_pending {
                if let Some(oldest) = self
                    .pending
                    .iter()
                    .min_by_key(|(_, a)| a.started)
                    .map(|(k, _)| *k)
                {
                    self.pending.remove(&oldest);
                }
            }
        }

        let assembly = self
            .pending
            .entry(header.message_id)
            .or_insert_with(|| PendingAssembly {
                fragments: vec![None; header.fragment_count as usize],
                total: header.fragment_count,
                received: 0,
                started: Instant::now(),
            });

        let idx = header.fragment_index as usize;
        if idx >= assembly.fragments.len() {
            return Err(NetError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "fragment index out of range",
            )));
        }

        if assembly.fragments[idx].is_none() {
            assembly.fragments[idx] = Some(payload);
            assembly.received += 1;
        }

        if assembly.received == assembly.total {
            let assembly = self.pending.remove(&header.message_id).unwrap();
            let mut message = Vec::new();
            for frag in assembly.fragments {
                message.extend_from_slice(&frag.unwrap());
            }
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// Remove timed-out pending assemblies.
    pub fn cleanup_stale(&mut self) {
        let timeout = self.timeout;
        self.pending
            .retain(|_, assembly| assembly.started.elapsed() < timeout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_assemblies_are_bounded_under_flood() {
        // Cap at 8; stream the first fragment of 100 distinct, never-completed
        // multi-fragment messages. Pending must never exceed the cap.
        let mut a = FragmentAssembler::with_max_pending(Duration::from_secs(60), 8);
        for id in 0..100u16 {
            let header = FragmentHeader {
                message_id: id,
                fragment_index: 0,
                fragment_count: 4,
            };
            let mut data = header.encode().to_vec();
            data.extend_from_slice(b"payload");
            let out = a.receive_fragment(&data).unwrap();
            assert!(out.is_none());
        }
        assert!(
            a.pending.len() <= 8,
            "pending grew unbounded to {}",
            a.pending.len()
        );
    }

    #[test]
    fn fragment_and_reassemble() {
        let mut assembler = FragmentAssembler::new(Duration::from_secs(5));

        let data: Vec<u8> = (0..200).map(|i| i as u8).collect();
        let fragments = assembler.fragment(&data, 50);
        assert!(fragments.len() > 1);

        let mut recv_assembler = FragmentAssembler::new(Duration::from_secs(5));
        let mut result = None;
        for frag in &fragments {
            result = recv_assembler.receive_fragment(frag).unwrap();
        }

        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn single_fragment_roundtrip() {
        let mut assembler = FragmentAssembler::new(Duration::from_secs(5));

        let data = b"small";
        let fragments = assembler.fragment(data, 100);
        assert_eq!(fragments.len(), 1);

        let mut recv_assembler = FragmentAssembler::new(Duration::from_secs(5));
        let result = recv_assembler.receive_fragment(&fragments[0]).unwrap();
        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn out_of_order_assembly() {
        let mut assembler = FragmentAssembler::new(Duration::from_secs(5));

        let data: Vec<u8> = (0..200).map(|i| i as u8).collect();
        let fragments = assembler.fragment(&data, 50);
        assert!(fragments.len() > 1);

        let mut recv_assembler = FragmentAssembler::new(Duration::from_secs(5));

        let mut result = None;
        for frag in fragments.iter().rev() {
            result = recv_assembler.receive_fragment(frag).unwrap();
        }

        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn stale_cleanup() {
        let mut assembler = FragmentAssembler::new(Duration::from_millis(0));

        let data: Vec<u8> = (0..200).map(|i| i as u8).collect();
        let fragments = assembler.fragment(&data, 50);

        let mut recv_assembler = FragmentAssembler::new(Duration::from_millis(0));
        recv_assembler.receive_fragment(&fragments[0]).unwrap();
        assert_eq!(recv_assembler.pending.len(), 1);

        recv_assembler.cleanup_stale();
        assert_eq!(recv_assembler.pending.len(), 0);
    }

    #[test]
    fn empty_message_fragment() {
        let mut assembler = FragmentAssembler::new(Duration::from_secs(5));

        let data = b"";
        let fragments = assembler.fragment(data, 100);
        assert_eq!(fragments.len(), 1);

        let mut recv_assembler = FragmentAssembler::new(Duration::from_secs(5));
        let result = recv_assembler.receive_fragment(&fragments[0]).unwrap();
        assert_eq!(result.unwrap(), data);
    }
}
