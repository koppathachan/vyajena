//! pi-runtime: Graceful stop. The real binary, stopped with `SIGTERM` (what
//! systemd sends) and `SIGINT` (Ctrl-C).

mod support;

use std::os::unix::process::ExitStatusExt;

use support::binary::{Home, admin_on};
use support::harness::Conn;
use support::messaging::{Stream, envelope};
use support::pb;
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;

/// What Alice's stream saw for the envelope in flight when the server
/// stopped: `Accepted` with its `server_seq`, or no answer.
async fn answer_before_end(stream: &mut Stream, msg_id: &[u8]) -> Option<u64> {
    loop {
        match stream.try_next().await {
            Ok(Some(Out::SendResult(result))) => {
                assert_eq!(result.msg_id, msg_id);
                match result.result {
                    Some(pb::send_result::Result::Accepted(accepted)) => {
                        return Some(accepted.server_seq);
                    }
                    other => panic!("expected Accepted or no answer, got {other:?}"),
                }
            }
            Ok(Some(_)) => continue,
            // The stream ended (cleanly or with a status) without an answer.
            Ok(None) | Err(_) => return None,
        }
    }
}

// spec: pi-runtime/graceful-stop
#[tokio::test]
async fn sigterm_mid_send_exits_zero_and_the_message_is_stored_or_resent() {
    let home = Home::new();
    let pi = home.start();
    let (_conn, _admin, mut device, mut stream) = admin_on(&pi).await;
    // A message before the stop, accepted as usual.
    let epoch = device.epoch();
    let first = stream
        .accepted(envelope(
            Kind::Application,
            epoch,
            device.message(b"before"),
        ))
        .await;

    // systemd stops the service while the next message is being accepted.
    let in_flight = envelope(Kind::Application, epoch, device.message(b"in flight"));
    stream.send_envelope(in_flight.clone()).await;
    pi.signal("TERM");
    let answered = answer_before_end(&mut stream, &in_flight.msg_id).await;
    let status = pi.wait().await;
    assert_eq!(status.code(), Some(0), "exit status {status:?}");
    assert_eq!(status.signal(), None);

    // After the restart Alice resends what had no answer; the Pi answers a
    // resend of a stored message with its original server_seq.
    let pi = home.start();
    let conn = Conn::open(pi.addr, pi.pin).await;
    device.authenticate(&conn).await;
    let mut stream = Stream::open(&conn, &device).await;
    let seq = stream.accepted(in_flight).await;
    assert!(seq > first, "{seq} after {first}");
    if let Some(answered) = answered {
        assert_eq!(seq, answered, "the accepted message was stored");
    }
    let after = stream
        .accepted(envelope(Kind::Application, epoch, device.message(b"after")))
        .await;
    assert_eq!(after, seq + 1);
}

// spec: pi-runtime/graceful-stop
#[tokio::test]
async fn sigterm_with_open_streams_exits_zero_and_keeps_the_pin() {
    let home = Home::new();
    let pi = home.start();
    let pin = pi.pin;
    // Idle open streams must not hold the stop up.
    let (_conn, _admin, _device, mut stream) = admin_on(&pi).await;
    pi.signal("TERM");
    let ended = stream.ended().await;
    assert!(
        matches!(&ended, Err(status) if status.code() == tonic::Code::Unavailable),
        "{ended:?}"
    );
    let status = pi.wait().await;
    assert_eq!(status.code(), Some(0), "exit status {status:?}");
    // The database was closed: SQLite removes the WAL file at a clean close.
    assert!(!home.path().join("pi.sqlite-wal").exists());

    let pi = home.start();
    assert_eq!(pi.pin, pin);
    assert!(
        pi.invite_code.is_none(),
        "the bootstrap invite is made once"
    );
}

// spec: pi-runtime/graceful-stop
#[tokio::test]
async fn ctrl_c_still_stops_cleanly() {
    let home = Home::new();
    let pi = home.start();
    pi.signal("INT");
    let status = pi.wait().await;
    assert_eq!(status.code(), Some(0), "exit status {status:?}");
}
