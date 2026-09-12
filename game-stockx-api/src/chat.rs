use crate::DBPool;
use crate::constants::CONNECTION_POOL_ERROR;
use crate::metrics::{CHAT_MESSAGES_SENT, CHAT_PERSISTENCE_ERRORS, WS_CONNECTIONS};
use actix::prelude::*;
use actix_rt::task::spawn_blocking;
use actix_web::{Error, HttpRequest, HttpResponse, web};
use actix_web_actors::ws;
use actix_web_actors::ws::ProtocolError;
use chrono::{DateTime, Utc};
use diesel::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use diesel::sql_types::{Array, BigInt, Bool, Integer, Jsonb, Nullable, Text, Timestamptz};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Message, Serialize, Deserialize, Debug, Clone, QueryableByName)]
#[rtype(result = "()")]
pub struct ClientMessage {
    #[diesel(sql_type = Integer)]
    pub id: i32,
    #[diesel(sql_type = Text)]
    pub sender: String,
    #[diesel(sql_type = Text)]
    pub recipient: String,
    #[diesel(sql_type = Text)]
    pub body: String,
    #[diesel(sql_type = Timestamptz)]
    pub created_at: DateTime<Utc>,
    #[diesel(sql_type = Bool)]
    pub read: bool,
    #[diesel(sql_type = Nullable<Timestamptz>)]
    pub read_at: Option<DateTime<Utc>>,
    #[diesel(sql_type = Nullable<Text>)]
    pub client_id: Option<String>,
}

#[derive(Message, Serialize, Clone)]
#[rtype(result = "()")]
#[serde(untagged)]
pub enum ServerEvent {
    Message(ClientMessage),
    Unread {
        r#type: &'static str,
        revision: i64,
        unread: serde_json::Value,
    },
    Read {
        r#type: &'static str,
        reader: String,
        ids: Vec<i32>,
        read_at: DateTime<Utc>,
    },
    Typing {
        r#type: &'static str,
        sender: String,
        typing: bool,
    },
    SendFailed {
        r#type: &'static str,
        client_id: Option<String>,
    },
    NewRequest {
        r#type: &'static str,
        request_id: i32,
        kind: String,
    },
    SessionRevoked {
        r#type: &'static str,
    },
    Presence {
        r#type: &'static str,
        online: Vec<String>,
    },
}

#[derive(Message)]
#[rtype(result = "()")]
pub enum ChatCommand {
    NotifyAdmins {
        request_id: i32,
        kind: String,
    },
    RemoveUser {
        login: String,
    },
    Connect {
        login: String,
        addr: Recipient<ServerEvent>,
    },
    Disconnect {
        login: String,
        addr: Recipient<ServerEvent>,
    },
    ReadMessages {
        reader: String,
        ids: Vec<i32>,
    },
    Typing {
        sender: String,
        recipient: String,
        typing: bool,
    },
    SendMessage {
        client_id: Option<String>,
        sender: String,
        recipient: String,
        body: String,
    },
}

pub struct ChatServer {
    sessions: HashMap<String, HashSet<Recipient<ServerEvent>>>,
    db_pool: r2d2::Pool<ConnectionManager<PgConnection>>,
}

impl ChatServer {
    fn deliver(&self, login: &str, event: ServerEvent) {
        if let Some(sessions) = self.sessions.get(login) {
            for session in sessions {
                session.do_send(event.clone());
            }
        }
    }

    fn broadcast_presence(&self) {
        let mut online: Vec<String> = self.sessions.keys().cloned().collect();
        online.sort();
        let event = ServerEvent::Presence {
            r#type: "presence",
            online,
        };
        for session in self.sessions.values().flatten() {
            session.do_send(event.clone());
        }
    }

    pub fn new(db_pool: r2d2::Pool<ConnectionManager<PgConnection>>) -> ChatServer {
        WS_CONNECTIONS.set(0);
        ChatServer {
            sessions: HashMap::new(),
            db_pool,
        }
    }
}

impl Actor for ChatServer {
    type Context = Context<Self>;
}

impl Handler<ChatCommand> for ChatServer {
    type Result = ();

    fn handle(&mut self, msg: ChatCommand, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            ChatCommand::NotifyAdmins { request_id, kind } => {
                let pool = self.db_pool.clone();
                ctx.spawn(
                    async move {
                        spawn_blocking(move || {
                            #[derive(QueryableByName)]
                            struct AdminLogin {
                                #[diesel(sql_type = Text)]
                                user_login: String,
                            }
                            let mut conn = pool.get().map_err(|e| e.to_string())?;
                            diesel::sql_query("SELECT user_login FROM users WHERE is_admin=true")
                                .load::<AdminLogin>(&mut conn)
                                .map(|rows| {
                                    rows.into_iter().map(|r| r.user_login).collect::<Vec<_>>()
                                })
                                .map_err(|e| e.to_string())
                        })
                        .await
                    }
                    .into_actor(self)
                    .map(move |result, server, _| match result {
                        Ok(Ok(logins)) => {
                            let event = ServerEvent::NewRequest {
                                r#type: "new_request",
                                request_id,
                                kind,
                            };
                            for login in logins {
                                if let Some(sessions) = server.sessions.get(&login) {
                                    for session in sessions {
                                        session.do_send(event.clone());
                                    }
                                }
                            }
                        }
                        _ => log::warn!("Could not notify administrators of a new request"),
                    }),
                );
            }

            ChatCommand::RemoveUser { login } => {
                if let Some(sessions) = self.sessions.remove(&login) {
                    WS_CONNECTIONS.dec();
                    for session in sessions {
                        session.do_send(ServerEvent::SessionRevoked {
                            r#type: "session_revoked",
                        });
                    }
                    self.broadcast_presence();
                }
                let pool = self.db_pool.clone();
                let logins: Vec<String> = self.sessions.keys().cloned().collect();
                ctx.spawn(
                    async move {
                        spawn_blocking(move || {
                            let mut conn = pool.get().ok()?;
                            Some(
                                logins
                                    .into_iter()
                                    .filter_map(|login| {
                                        unread_snapshot(&mut conn, &login)
                                            .ok()
                                            .map(|snapshot| (login, snapshot))
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .await
                    }
                    .into_actor(self)
                    .map(|result, server, _| {
                        if let Ok(Some(snapshots)) = result {
                            for (login, snapshot) in snapshots {
                                server.deliver(&login, snapshot.event());
                            }
                        }
                    }),
                );
            }
            ChatCommand::Connect { login, addr } => {
                println!("ConnectedWS: {}", login.clone());

                let was_online = self.sessions.contains_key(&login);

                self.sessions.entry(login).or_default().insert(addr);

                if !was_online {
                    WS_CONNECTIONS.inc();
                }
                self.broadcast_presence();
            }
            ChatCommand::Disconnect { login, addr } => {
                if let Some(sessions) = self.sessions.get_mut(&login) {
                    sessions.remove(&addr);
                    if sessions.is_empty() {
                        self.sessions.remove(&login);
                        WS_CONNECTIONS.dec();
                        self.broadcast_presence();
                    }
                }
                println!("DisconnectedWS: {}", login);
            }
            ChatCommand::Typing {
                sender,
                recipient,
                typing,
            } => {
                if sender != recipient {
                    self.deliver(
                        &recipient,
                        ServerEvent::Typing {
                            r#type: "typing",
                            sender,
                            typing,
                        },
                    );
                }
            }
            ChatCommand::ReadMessages { reader, ids } => {
                if ids.is_empty() || ids.len() > 100 {
                    return;
                }
                let pool = self.db_pool.clone();
                ctx.spawn(async move {
                    spawn_blocking(move || -> Result<_,diesel::result::Error> {
                        let mut conn=pool.get().map_err(|_|diesel::result::Error::NotFound)?;
                        conn.transaction(|conn| {
                            lock_receipts(conn,&reader)?;
                            let rows=diesel::sql_query("UPDATE messages SET read=true,read_at=now() WHERE recipient_login=$1 AND id=ANY($2) AND NOT read RETURNING id,sender_login AS sender,recipient_login AS recipient,body,created_at,read,read_at,client_id")
                                .bind::<Text,_>(&reader).bind::<Array<Integer>,_>(ids).load::<ClientMessage>(conn)?;
                            if !rows.is_empty() { bump_receipts(conn,&reader)?; }
                            let snapshot=unread_snapshot(conn,&reader)?;
                            Ok((reader,rows,snapshot))
                        })
                    }).await
                }.into_actor(self).map(|result,server,_| {
                    if let Ok(Ok((reader,rows,snapshot)))=result {
                        let mut groups: HashMap<String,Vec<i32>>=HashMap::new();
                        let at=rows.first().and_then(|row|row.read_at).unwrap_or_else(Utc::now);
                        for row in rows { groups.entry(row.sender).or_default().push(row.id); }
                        for (sender,ids) in groups {
                            let event=ServerEvent::Read { r#type:"read",reader:reader.clone(),ids,read_at:at };
                            server.deliver(&sender,event.clone()); server.deliver(&reader,event);
                        }
                        server.deliver(&reader,snapshot.event());
                    } else { log::warn!("Could not save chat read receipt"); }
                }));
            }
            ChatCommand::SendMessage {
                sender,
                recipient,
                body,
                client_id,
            } => {
                let pool = self.db_pool.clone();
                let failed_sender = sender.clone();
                let failed_id = client_id.clone();
                ctx.spawn(async move {
                    spawn_blocking(move || -> Result<_,diesel::result::Error> {
                        let mut conn=pool.get().map_err(|_|diesel::result::Error::NotFound)?;
                        conn.transaction(|conn| {
                            lock_receipts(conn,&recipient)?;
                            let added=diesel::sql_query("INSERT INTO messages(sender_login,recipient_login,body,client_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING id,sender_login AS sender,recipient_login AS recipient,body,created_at,read,read_at,client_id")
                                .bind::<Text,_>(&sender).bind::<Text,_>(&recipient).bind::<Text,_>(&body).bind::<Nullable<Text>,_>(&client_id).get_result::<ClientMessage>(conn).optional()?;
                            let inserted=added.is_some();
                            let message=match added {Some(message)=>message,None=>diesel::sql_query("SELECT id,sender_login AS sender,recipient_login AS recipient,body,created_at,read,read_at,client_id FROM messages WHERE sender_login=$1 AND client_id=$2 AND recipient_login=$3 AND body=$4")
                                .bind::<Text,_>(&sender).bind::<Nullable<Text>,_>(&client_id).bind::<Text,_>(&recipient).bind::<Text,_>(&body).get_result::<ClientMessage>(conn)?};
                            if inserted { bump_receipts(conn,&recipient)?; }
                            let snapshot=unread_snapshot(conn,&recipient)?;
                            Ok((message,snapshot,inserted))
                        })
                    }).await
                }.into_actor(self).map(move |result,server,_| match result {
                    Ok(Ok((message,snapshot,inserted)))=> {
                        if inserted { CHAT_MESSAGES_SENT.inc(); }
                        let recipient=message.recipient.clone(); let sender=message.sender.clone();
                        server.deliver(&sender,ServerEvent::Message(message.clone()));
                        if sender!=recipient { server.deliver(&recipient,ServerEvent::Message(message)); }
                        server.deliver(&recipient,ServerEvent::Typing {r#type:"typing",sender,typing:false});
                        server.deliver(&recipient,snapshot.event());
                    }
                    _=> {CHAT_PERSISTENCE_ERRORS.inc();server.deliver(&failed_sender,ServerEvent::SendFailed{r#type:"send_failed",client_id:failed_id});log::warn!("Could not persist chat message");}
                }));
            }
        }
    }
}

pub struct ChatSession {
    pub login: String,
    pub addr: Addr<ChatServer>,
    disconnected: Arc<AtomicBool>,
    heartbeat: std::time::Instant,
    authenticated: bool,
    user_id: i32,
    last_typing: Option<std::time::Instant>,
}

#[derive(Deserialize)]
struct Authentication {
    r#type: String,
    token: String,
}

#[derive(Deserialize)]
struct OutgoingMessage {
    client_id: Option<String>,
    recipient: String,
    body: String,
}

impl ChatSession {
    fn reject(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.close(Some(ws::CloseReason {
            code: ws::CloseCode::Policy,
            description: Some("Authentication required".into()),
        }));
        ctx.stop();
    }
}

impl Actor for ChatSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // A socket is not a user session until the first frame passes JWT validation.
        ctx.run_later(std::time::Duration::from_secs(5), |session, ctx| {
            if !session.authenticated {
                session.reject(ctx);
            }
        });

        ctx.run_interval(std::time::Duration::from_secs(30), |session, ctx| {
            if session.heartbeat.elapsed() > std::time::Duration::from_secs(90) {
                ctx.stop();
                return;
            }
            if session.authenticated
                && !crate::auth::account_exists(session.user_id, &session.login)
            {
                session.reject(ctx);
                return;
            }
            ctx.ping(b"keep-alive");
        });
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        if self.authenticated && !self.disconnected.swap(true, Ordering::SeqCst) {
            self.addr.do_send(ChatCommand::Disconnect {
                login: self.login.clone(),
                addr: ctx.address().recipient(), // Передаем свой addr
            });
        }
    }
}

impl StreamHandler<Result<ws::Message, ProtocolError>> for ChatSession {
    fn handle(&mut self, msg: Result<ws::Message, ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                if !self.authenticated {
                    let claims = serde_json::from_str::<Authentication>(&text)
                        .ok()
                        .filter(|auth| auth.r#type == "authenticate")
                        .and_then(|auth| crate::auth::verify_jwt(&auth.token))
                        .filter(|claims| claims.exp > Utc::now().timestamp() as usize);
                    let Some(claims) = claims else {
                        self.reject(ctx);
                        return;
                    };
                    self.user_id = claims.uid;
                    self.login = claims.sub;
                    self.authenticated = true;
                    self.addr.do_send(ChatCommand::Connect {
                        login: self.login.clone(),
                        addr: ctx.address().recipient(),
                    });
                    ctx.text(
                        serde_json::json!({"type": "authenticated", "login": self.login})
                            .to_string(),
                    );
                    let remaining = claims.exp.saturating_sub(Utc::now().timestamp() as usize);
                    ctx.run_later(
                        std::time::Duration::from_secs(remaining as u64),
                        |session, ctx| session.reject(ctx),
                    );
                    return;
                }
                if !crate::auth::account_exists(self.user_id, &self.login) {
                    self.reject(ctx);
                    return;
                }
                let value = serde_json::from_str::<serde_json::Value>(&text).ok();
                match value
                    .as_ref()
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                {
                    Some("read") => {
                        if let Some(ids) = value
                            .as_ref()
                            .and_then(|v| v.get("ids"))
                            .and_then(|v| serde_json::from_value::<Vec<i32>>(v.clone()).ok())
                            .filter(|ids| ids.len() <= 100)
                        {
                            self.addr.do_send(ChatCommand::ReadMessages {
                                reader: self.login.clone(),
                                ids,
                            });
                        }
                    }
                    Some("typing") => {
                        match (
                            value
                                .as_ref()
                                .and_then(|v| v.get("recipient"))
                                .and_then(|v| v.as_str()),
                            value
                                .as_ref()
                                .and_then(|v| v.get("typing"))
                                .and_then(|v| v.as_bool()),
                        ) {
                            (Some(recipient), Some(typing))
                                if recipient.len() <= 200
                                    && (!typing
                                        || self.last_typing.is_none_or(|last| {
                                            last.elapsed() >= std::time::Duration::from_millis(800)
                                        })) =>
                            {
                                if typing {
                                    self.last_typing = Some(std::time::Instant::now());
                                }
                                self.addr.do_send(ChatCommand::Typing {
                                    sender: self.login.clone(),
                                    recipient: recipient.into(),
                                    typing,
                                });
                            }
                            _ => {}
                        }
                    }
                    None | Some("message") => {
                        if let Ok(parsed) = serde_json::from_str::<OutgoingMessage>(&text) {
                            if !parsed.body.trim().is_empty()
                                && parsed.body.len() <= 16000
                                && parsed.recipient.len() <= 200
                                && parsed
                                    .client_id
                                    .as_ref()
                                    .is_none_or(|id| !id.is_empty() && id.len() <= 64)
                            {
                                self.addr.do_send(ChatCommand::SendMessage {
                                    sender: self.login.clone(),
                                    recipient: parsed.recipient,
                                    body: parsed.body,
                                    client_id: parsed.client_id,
                                });
                            } else {
                                ctx.text(serde_json::json!({"type":"send_failed","client_id":parsed.client_id}).to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(ws::Message::Ping(msg)) => {
                self.heartbeat = std::time::Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = std::time::Instant::now();
            }
            Ok(ws::Message::Close(reason)) => {
                println!("WebSocket closed: {:?}", reason);
                ctx.stop();
            }
            Ok(ws::Message::Binary(_)) => {
                // Игнорируем или логируем
            }
            Ok(ws::Message::Continuation(_)) => {
                // Игнорируем или логируем
            }
            Ok(ws::Message::Nop) => {
                // Ничего не делаем (это heartbeat от actix, можно игнорировать)
            }
            Err(e) => {
                println!("WebSocket error: {:?}", e);
                ctx.stop();
            }
        }
    }
}

impl Handler<ServerEvent> for ChatSession {
    type Result = ();

    fn handle(&mut self, msg: ServerEvent, ctx: &mut Self::Context) {
        if matches!(msg, ServerEvent::SessionRevoked { .. }) {
            self.reject(ctx);
            return;
        }
        if let Ok(text) = serde_json::to_string(&msg) {
            ctx.text(text);
        }
    }
}

// === HTTP entrypoint для WS ===
pub async fn chat_ws(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<ChatServer>>,
) -> Result<HttpResponse, Error> {
    let session = ChatSession {
        login: String::new(),
        addr: srv.get_ref().clone(),
        disconnected: Arc::new(AtomicBool::new(false)),
        heartbeat: std::time::Instant::now(),
        authenticated: false,
        user_id: 0,
        last_typing: None,
    };
    ws::start(session, &req, stream)
}

#[derive(Deserialize)]
pub struct MessageQuery {
    companion: String,
}

#[get("/messages")]
async fn get_my_messages(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<MessageQuery>,
) -> HttpResponse {
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let my_login = claims.sub;
    let other_login = query.companion.clone();

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
       SELECT id, sender_login as sender, recipient_login as recipient, body, created_at, read, read_at, client_id
        FROM messages
        WHERE (sender_login = $1 AND recipient_login = $2)
           OR (sender_login = $2 AND recipient_login = $1)
        ORDER BY id ASC
    "#;

    let messages = match diesel::sql_query(query)
        .bind::<Text, _>(&my_login)
        .bind::<Text, _>(&other_login)
        .load::<ClientMessage>(conn)
    {
        Ok(results) => results,
        Err(err) => {
            eprintln!("DB error: {:?}", err);
            return HttpResponse::InternalServerError().body("Error fetching messages");
        }
    };

    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(messages)
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct DialogDto {
    #[diesel(sql_type = Text)]
    pub companion: String,
    #[diesel(sql_type = Text)]
    pub last_message: String,
    #[diesel(sql_type = Timestamptz)]
    pub last_message_time: DateTime<Utc>,
}

#[get("/dialogs")]
async fn get_my_dialogs(pool: web::Data<DBPool>, req: HttpRequest) -> HttpResponse {
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let login = claims.sub;
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
        SELECT DISTINCT ON (companion)
            CASE 
                WHEN sender_login = $1 THEN recipient_login
                ELSE sender_login
            END AS companion,
            body AS last_message,
            created_at AS last_message_time
        FROM messages
        WHERE sender_login = $1 OR recipient_login = $1
        ORDER BY companion, created_at DESC
    "#;

    let dialogs = match diesel::sql_query(query)
        .bind::<Text, _>(&login)
        .load::<DialogDto>(conn)
    {
        Ok(results) => results,
        Err(err) => {
            eprintln!("DB error: {:?}", err);
            return HttpResponse::InternalServerError().body("Error fetching dialogs");
        }
    };

    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(dialogs)
}

#[cfg(test)]
mod presence_tests {
    use super::*;
    use std::sync::Mutex;

    struct Observer(Arc<Mutex<Vec<serde_json::Value>>>);
    impl Actor for Observer {
        type Context = Context<Self>;
    }
    impl Handler<ServerEvent> for Observer {
        type Result = ();
        fn handle(&mut self, event: ServerEvent, _: &mut Context<Self>) {
            self.0
                .lock()
                .unwrap()
                .push(serde_json::to_value(event).unwrap());
        }
    }

    #[actix_rt::test]
    async fn presence_tracks_users_across_multiple_tabs() {
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .min_idle(Some(0))
            .build_unchecked(ConnectionManager::<PgConnection>::new(
                "postgres://localhost/unused",
            ));
        let server = ChatServer::new(pool).start();
        let events = Arc::new(Mutex::new(Vec::new()));
        let first = Observer(events.clone()).start().recipient();
        let second = Observer(events.clone()).start().recipient();
        let bob = Observer(events.clone()).start().recipient();
        for (login, addr) in [
            ("alice", first.clone()),
            ("alice", second.clone()),
            ("bob", bob),
        ] {
            server
                .send(ChatCommand::Connect {
                    login: login.into(),
                    addr,
                })
                .await
                .unwrap();
        }
        server
            .send(ChatCommand::Disconnect {
                login: "alice".into(),
                addr: first,
            })
            .await
            .unwrap();
        actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            events.lock().unwrap().last().unwrap()["online"],
            serde_json::json!(["alice", "bob"])
        );
        server
            .send(ChatCommand::Disconnect {
                login: "alice".into(),
                addr: second,
            })
            .await
            .unwrap();
        actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            events.lock().unwrap().last().unwrap(),
            &serde_json::json!({"type": "presence", "online": ["bob"]})
        );
    }

    #[actix_rt::test]
    async fn messages_keep_the_existing_wire_format() {
        let message = ClientMessage {
            id: 1,
            read: false,
            read_at: None,
            client_id: None,
            sender: "alice".into(),
            recipient: "bob".into(),
            body: "Hi".into(),
            created_at: Utc::now(),
        };
        assert_eq!(
            serde_json::to_value(ServerEvent::Message(message.clone())).unwrap(),
            serde_json::to_value(message).unwrap()
        );
    }
}

#[derive(Serialize, QueryableByName)]
struct UnreadSnapshot {
    #[diesel(sql_type=BigInt)]
    revision: i64,
    #[diesel(sql_type=Jsonb)]
    unread: serde_json::Value,
}
impl UnreadSnapshot {
    fn event(self) -> ServerEvent {
        ServerEvent::Unread {
            r#type: "unread",
            revision: self.revision,
            unread: self.unread,
        }
    }
}
fn lock_receipts(conn: &mut PgConnection, login: &str) -> QueryResult<()> {
    diesel::sql_query(
        "INSERT INTO chat_receipt_state(user_login) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind::<Text, _>(login)
    .execute(conn)?;
    diesel::sql_query("SELECT user_login FROM chat_receipt_state WHERE user_login=$1 FOR UPDATE")
        .bind::<Text, _>(login)
        .execute(conn)?;
    Ok(())
}
fn bump_receipts(conn: &mut PgConnection, login: &str) -> QueryResult<()> {
    diesel::sql_query("UPDATE chat_receipt_state SET revision=revision+1 WHERE user_login=$1")
        .bind::<Text, _>(login)
        .execute(conn)?;
    Ok(())
}
fn unread_snapshot(conn: &mut PgConnection, login: &str) -> QueryResult<UnreadSnapshot> {
    diesel::sql_query("SELECT COALESCE((SELECT revision FROM chat_receipt_state WHERE user_login=$1),0)::bigint AS revision,COALESCE((SELECT jsonb_object_agg(sender_login,total) FROM (SELECT sender_login,count(*) AS total FROM messages WHERE recipient_login=$1 AND NOT read GROUP BY sender_login) counts),'{}'::jsonb) AS unread")
        .bind::<Text,_>(login).get_result(conn)
}
#[get("/chat/unread")]
async fn get_unread(
    pool: web::Data<DBPool>,
    req: HttpRequest,
) -> Result<HttpResponse, crate::admin::AdminError> {
    let user =
        crate::auth::authenticated_claims(&req).ok_or(crate::admin::AdminError::Unauthorized)?;
    let snapshot =
        crate::admin::db(pool, move |conn| Ok(unread_snapshot(conn, &user.sub)?)).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(snapshot))
}
