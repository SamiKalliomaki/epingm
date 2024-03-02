use oneshot::TryRecvError;
use pnet::packet::icmp;
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::{icmpv6, MutablePacket, Packet};
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::{Ipv4, Ipv6};
use pnet::transport::{icmp_packet_iter, TransportSender};
use pnet::util;
use rand::{thread_rng, RngCore};
use std::net::IpAddr;
use std::time::{Duration, Instant};
use std::{thread, vec, io};

#[derive(Debug, Clone)]
pub struct PingResult {
    pub latency: Duration,
    pub reply_size: usize,
}

pub struct VolleyInfo {
    pub results: Vec<Option<PingResult>>,
    pub sent: usize,
    pub received: usize,
    pub lost: usize,
}

pub enum VolleyResult {
    Success(VolleyInfo),
    Error(String),
}

pub fn measure_volley(
    target: IpAddr,
    count: usize,
    size: usize,
    interval: Duration,
    timeout: Duration,
) -> VolleyResult {
    let protocol = match target {
        IpAddr::V4(_) => Layer4(Ipv4(IpNextHeaderProtocols::Icmp)),
        IpAddr::V6(_) => Layer4(Ipv6(IpNextHeaderProtocols::Icmpv6)),
    };

    let ip_header_size = match target {
        IpAddr::V4(_) => 20,
        IpAddr::V6(_) => 40,
    };

    // 14 bytes for ethernet frame header
    // ip_header_size bytes for IP header
    // 8 bytes for ICMP header
    // size bytes for payload
    let packet_size = 14 + ip_header_size + 8 + size;

    let (mut tx, rx) = match pnet::transport::transport_channel(packet_size * 16, protocol) {
        Ok((tx, rx)) => (tx, rx),
        Err(e) => return VolleyResult::Error(format!("Failed to create transport channel: {}", e)),
    };
    let (stop_signal_tx, stop_signal_rx) = oneshot::channel();

    let identifier = rand::random::<u16>();
    let receiver = thread::spawn(move || {
        return receive_ipv4(rx, count, timeout, target, identifier, stop_signal_rx);
    });

    let mut volley_info = VolleyInfo {
        results: vec![None; count],
        sent: 0,
        received: 0,
        lost: 0,
    };
    let mut request_send_times: Vec<Instant> = Vec::new();

    let mut next_packet = Instant::now();
    for seq in 0..count {
        request_send_times.push(Instant::now());
        let send_result = match target {
            IpAddr::V4(_) => send_ipv4_echo_request(&mut tx, target, size, identifier, seq as u16),
            IpAddr::V6(_) => send_ipv6_echo_request(&mut tx, target, size, identifier, seq as u16),
        };
        match send_result {
            Err(e) => {
                eprintln!("Failed to send packet: {}", e);
            }
            Ok(_) => {
                volley_info.sent += 1;
            }
        }


        next_packet += interval;
        thread::sleep(next_packet - Instant::now());
    }

    _ = stop_signal_tx.send(Instant::now() + timeout);
    let results = receiver.join().expect("Failed to join receiver thread");

    for result in results {
        let seq = result.seq as usize;
        if seq >= count {
            eprintln!(
                "Received packet with invalid sequence number: {}",
                result.seq
            );
            continue;
        }
        let latency = result.time - request_send_times[seq];
        if latency > timeout {
            continue;
        }

        if let Some(_) = volley_info.results[seq] {
            eprintln!("Received duplicate packet with sequence number: {}", result.seq);
            continue;
        }

        volley_info.received += 1;
        volley_info.results[seq] = Some(PingResult {
            latency,
            reply_size: result.size,
        });
    }
    volley_info.lost = count - volley_info.received;

    return VolleyResult::Success(volley_info);
}

fn send_ipv4_echo_request(
    tx: &mut TransportSender,
    target: IpAddr,
    size: usize,
    identifier: u16,
    seq: u16,
) -> io::Result<()> {
    let packet_size = 8 + size;
    let mut packet = vec![0; packet_size];

    let mut icmp_packet = icmp::echo_request::MutableEchoRequestPacket::new(&mut packet)
        .expect("Failed to create ICMP echo request packet");

    icmp_packet.set_icmp_type(icmp::IcmpTypes::EchoRequest);
    icmp_packet.set_identifier(identifier);
    icmp_packet.set_sequence_number(seq);
    thread_rng().fill_bytes(icmp_packet.payload_mut());

    let checksum = util::checksum(&icmp_packet.packet(), 1);
    icmp_packet.set_checksum(checksum);

    match tx.send_to(icmp_packet, target) {
        Err(e) => return Err(e),
        Ok(_) => {}
    }

    Ok(())
}

fn send_ipv6_echo_request(
    tx: &mut TransportSender,
    target: IpAddr,
    size: usize,
    identifier: u16,
    seq: u16,
) -> io::Result<()> {
    let packet_size = 8 + size;
    let mut packet = vec![0; packet_size];
    let mut icmp_packet = icmpv6::echo_request::MutableEchoRequestPacket::new(&mut packet)
        .expect("Failed to create ICMP packet");

    icmp_packet.set_icmpv6_type(icmpv6::Icmpv6Types::EchoRequest);
    icmp_packet.set_identifier(identifier);
    icmp_packet.set_sequence_number(seq);
    thread_rng().fill_bytes(icmp_packet.payload_mut());

    let checksum = util::checksum(&icmp_packet.packet(), 1);
    icmp_packet.set_checksum(checksum);

    match tx.send_to(icmp_packet, target) {
        Err(e) => return Err(e),
        Ok(_) => {}
    }

    Ok(())
}

struct ReplyResult {
    seq: u16,
    time: Instant,
    size: usize,
}

fn receive_ipv4(
    mut rx: pnet::transport::TransportReceiver,
    count: usize,
    timeout: Duration,
    target: IpAddr,
    identifier: u16,
    stop_signal: oneshot::Receiver<Instant>,
) -> Vec<ReplyResult> {
    let mut results: Vec<ReplyResult> = Vec::new();
    let mut iter = icmp_packet_iter(&mut rx);
    let mut stop_time: Option<Instant> = None;

    results.reserve(count);

    loop {
        if results.len() >= count {
            break;
        }

        if stop_time == None {
            stop_time = match stop_signal.try_recv() {
                Ok(stop_time) => Some(stop_time),
                Err(TryRecvError::Empty) => None,
                Err(e) => panic!("Unexpected error receiving {}", e),
            };
        }
        let timeout = match stop_time {
            Some(stop_time) => {
                let now = Instant::now();
                if now >= stop_time {
                    break;
                }
                stop_time - now
            }
            None => timeout,
        };

        match iter.next_with_timeout(timeout) {
            Ok(Some((packet, addr))) => {
                if addr != target {
                    continue;
                }
                if packet.get_icmp_type() != icmp::IcmpTypes::EchoReply {
                    break;
                }
                let icmp_reply = match icmp::echo_reply::EchoReplyPacket::new(packet.packet()) {
                    Some(reply) => reply,
                    None => continue,
                };
                if icmp_reply.get_checksum() != util::checksum(&icmp_reply.packet(), 1) {
                    eprintln!("Received packet with invalid checksum");
                    continue;
                }
                if icmp_reply.get_identifier() != identifier {
                    continue;
                }

                results.push(ReplyResult {
                    seq: icmp_reply.get_sequence_number(),
                    time: Instant::now(),
                    size: icmp_reply.payload().len(),
                });
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Error receiving packet: {}", e);
                return results;
            }
        }
    }

    return results;
}
