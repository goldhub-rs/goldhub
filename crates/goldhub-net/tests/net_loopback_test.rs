use goldhub_net::connection::{ConnectionState, NetConnection};
use goldhub_net::protocol::oob::OobMessage;
use std::time::{Duration, Instant};

#[test]
fn test_loopback_oob_handshake() {
    let mut client = NetConnection::new(64 * 1024).expect("create client connection");
    let server = NetConnection::new(64 * 1024).expect("create server connection");

    let server_addr = server.local_addr();
    let client_addr = client.local_addr();

    client.set_remote(server_addr);
    assert_eq!(client.state(), ConnectionState::Challenging);

    // 1. Client sends getchallenge
    let req = OobMessage::ChallengeRequest;
    client.send_oob(&req).expect("send challenge request");

    // 2. Server receives getchallenge
    std::thread::sleep(Duration::from_millis(5));
    let mut buf = [0u8; 1024];
    let (len, from) = server
        .transport()
        .recv(&mut buf)
        .expect("recv")
        .expect("datagram pending");

    assert_eq!(from, client_addr);
    let decoded = OobMessage::decode(&buf[..len])
        .expect("decode oob")
        .expect("valid oob");
    assert_eq!(decoded, OobMessage::ChallengeRequest);

    // 3. Server responds with challenge 0x12345678
    let resp = OobMessage::ChallengeResponse {
        challenge: 0x1234_5678,
    };
    let mut resp_buf = [0u8; 1024];
    let resp_len = resp.encode(&mut resp_buf).expect("encode resp");
    server
        .transport()
        .send(&resp_buf[..resp_len], client_addr)
        .expect("send challenge response");

    // 4. Client receives response
    std::thread::sleep(Duration::from_millis(5));
    let (len, from) = client
        .transport()
        .recv(&mut buf)
        .expect("recv")
        .expect("datagram pending");
    assert_eq!(from, server_addr);

    let decoded = OobMessage::decode(&buf[..len])
        .expect("decode oob")
        .expect("valid oob");
    assert_eq!(
        decoded,
        OobMessage::ChallengeResponse {
            challenge: 0x1234_5678
        }
    );
}

#[test]
fn test_loopback_netchannel_packets_and_jitter_buffer() {
    let mut client = NetConnection::new(64 * 1024).expect("create client connection");
    let mut server = NetConnection::new(64 * 1024).expect("create server connection");

    let server_addr = server.local_addr();
    client.set_remote(server_addr);
    client.set_state(ConnectionState::Active);

    let now_micros = 1_000_000;

    // Send 5 sequenced packets from client to server
    for i in 1..=5 {
        let payload = format!("ClientFrame_{i}").into_bytes();
        client
            .send_packet(&payload, false)
            .expect("send client packet");
    }

    std::thread::sleep(Duration::from_millis(10));

    // Server pumps packets into ring and jitter buffer
    let mut received_count = 0;
    let start_time = Instant::now();

    while received_count < 5 && start_time.elapsed() < Duration::from_millis(500) {
        if let Some(pkt) = server.pump_incoming(now_micros).expect("pump server") {
            received_count += 1;
            assert_eq!(pkt.sequence, received_count);
            let text = String::from_utf8(pkt.payload).expect("valid utf8");
            assert_eq!(text, format!("ClientFrame_{received_count}"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(received_count, 5);
}
