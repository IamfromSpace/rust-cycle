use crate::db_session_to_fit;
use crate::telemetry_db::TelemetryDb;
use std::{mem, sync::Arc, thread, thread::JoinHandle, time::Duration};
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
                        let response = if request.url() == "/workouts/latest.fit" {
                            if request.method() == &Method::Get {
                                match db.get_most_recent_session().unwrap() {
                                    Some(most_recent_session) => {
                                        session = db_session_to_fit(&db, most_recent_session);
                                        Response::new(
                                            StatusCode(200),
                                            // TODO; Header for next most recent
                                            vec![Header::from_bytes(
                                                &b"Content-Type"[..],
                                                &b"application/vnd.ant.fit"[..],
                                            )
                                            .unwrap()],
                                            &session[..],
                                            None,
                                            None,
                                        )
                                    }
                                    None => {
                                        // The rare case where there are no
                                        // recorded workouts yet
                                        Response::new(StatusCode(404), vec![], &[][..], None, None)
                                    }
                                }
                            } else {
                                Response::new(StatusCode(405), vec![], &[][..], None, None)
                            }
                        } else {
                            Response::new(StatusCode(404), vec![], &[][..], None, None)
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
