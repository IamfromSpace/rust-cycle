use crate::db_sessions_to_fit;
use crate::telemetry_db::TelemetryDb;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::digit1,
    combinator::map,
    sequence::{pair, preceded, terminated},
    IResult,
};
use std::{mem, str::FromStr, sync::Arc, thread, thread::JoinHandle, time::Duration};
use tiny_http::{Header, Method, Response, Server, StatusCode};

pub struct TelemetryServer {
    running: Option<Arc<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl TelemetryServer {
    pub fn new(db: TelemetryDb) -> TelemetryServer {
        let running_for_thread = Arc::new(());
        let running = Some(running_for_thread.clone());
        let join_handle = Some(thread::spawn(move || {
            let server = Server::http("0.0.0.0:8080").unwrap();
            loop {
                // Every 100 millis, we check to see if the server should
                // continue running
                match server.recv_timeout(Duration::from_millis(100)).unwrap() {
                    None => {
                        // If the thread is  the last owner of the Arc, then
                        // there are no more interested parties and we terminate
                        if Arc::strong_count(&running_for_thread) <= 1 {
                            break;
                        }
                    }
                    // TODO: Reduce this awful nesting
                    Some(request) => {
                        #[allow(unused_assignments)]
                        let mut session: Vec<u8> = vec![];
                        // TODO: Some sort of simple auth (maybe a random pin on
                        // device?)
                        // TODO: Handle more than just the latest
                        let response = match parse_url(request.url()) {
                            Ok(url) => {
                                if request.method() == &Method::Get {
                                    let key = match url {
                                        (_, UrlKey::Latest) => {
                                            db.get_most_recent_session().unwrap().map(|k| (k, k))
                                        }
                                        (_, UrlKey::Key(k)) => Some((k, k)),
                                        (_, UrlKey::KeyRange((a, b))) => Some((a, b)),
                                    };
                                    match key {
                                        Some((a, b)) => {
                                            let o_session_keys =
                                                db.sessions_between_inclusive(a, b).unwrap();

                                            match o_session_keys {
                                                None => Response::new(
                                                    StatusCode(404),
                                                    vec![],
                                                    &[][..],
                                                    None,
                                                    None,
                                                ),
                                                Some(session_keys) => {
                                                    session = db_sessions_to_fit(
                                                        &db,
                                                        session_keys.into_iter(),
                                                    )
                                                    // TODO: 500
                                                    .unwrap();
                                                    let mut r = Response::new(
                                                        StatusCode(200),
                                                        // TODO; Header for next most recent
                                                        vec![
                                                            Header::from_bytes(
                                                                &b"Content-Type"[..],
                                                                &b"application/vnd.ant.fit"[..],
                                                            )
                                                            .unwrap(),
                                                            Header::from_bytes(
                                                                &b"Session-Key"[..],
                                                                // TODO: This is a coupling
                                                                if a == b {
                                                                    format!("{:?}-{:?}", a, b)
                                                                } else {
                                                                    format!("{:?}", a)
                                                                },
                                                            )
                                                            .unwrap(),
                                                        ],
                                                        &session[..],
                                                        None,
                                                        None,
                                                    );
                                                    if let Ok(Some(key)) =
                                                        db.get_previous_session(a)
                                                    {
                                                        r.add_header(
                                                            Header::from_bytes(
                                                                &b"Previous-Session-Key"[..],
                                                                format!("{:?}", key),
                                                            )
                                                            .unwrap(),
                                                        )
                                                    }
                                                    r
                                                }
                                            }
                                        }
                                        None => {
                                            // The rare case where there are no
                                            // recorded workouts yet
                                            Response::new(
                                                StatusCode(404),
                                                vec![],
                                                &[][..],
                                                None,
                                                None,
                                            )
                                        }
                                    }
                                } else {
                                    Response::new(StatusCode(405), vec![], &[][..], None, None)
                                }
                            }
                            Err(_) => Response::new(StatusCode(404), vec![], &[][..], None, None),
                        };
                        request.respond(response).unwrap();
                    }
                }
            }
        }));
        TelemetryServer {
            running,
            join_handle,
        }
    }
}

impl Drop for TelemetryServer {
    fn drop(&mut self) {
        // Drop the Arc immediately so the owner count is 1
        mem::replace(&mut self.running, None);
        if let Some(jh) = mem::replace(&mut self.join_handle, None) {
            jh.join().unwrap();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UrlKey {
    Latest,
    Key(u64),
    KeyRange((u64, u64)),
}

// TODO: Terminate
// TODO: This is a bit silly not to first put this through a standard URL parser
// that would first break it into components (which _then_ could be more
// thoroughly parsed).
fn parse_url(i: &str) -> IResult<&str, UrlKey> {
    terminated(
        preceded(
            tag("/workouts/"),
            alt((
                map(tag("latest"), |_| UrlKey::Latest),
                map(
                    pair(
                        map(digit1, |s| u64::from_str(s).unwrap()),
                        preceded(tag("-"), map(digit1, |s| u64::from_str(s).unwrap())),
                    ),
                    |t| UrlKey::KeyRange(t),
                ),
                map(digit1, |s| UrlKey::Key(u64::from_str(s).unwrap())),
            )),
        ),
        tag(".fit"),
    )(i)
}

#[cfg(test)]
mod tests {
    use super::parse_url;
    use super::UrlKey;

    #[test]
    fn parse_url_latest() {
        assert_eq!(parse_url("/workouts/latest.fit"), Ok(("", UrlKey::Latest)))
    }

    #[test]
    fn parse_url_key() {
        assert_eq!(parse_url("/workouts/1234.fit"), Ok(("", UrlKey::Key(1234))))
    }

    #[test]
    fn parse_url_key_range() {
        assert_eq!(
            parse_url("/workouts/1234-9382.fit"),
            Ok(("", UrlKey::KeyRange((1234, 9382))))
        )
    }
}
