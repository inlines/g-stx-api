use actix::prelude::*;
use actix_web::{Error, HttpRequest, HttpResponse, web};
use actix_web_actors::ws;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager};
use diesel::PgConnection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use actix_web_actors::ws::ProtocolError;
use diesel::sql_types::{Text, Timestamptz};
use crate::constants::{CONNECTION_POOL_ERROR};
use crate::auth::{verify_jwt};
use actix_web::http::header;
use crate::{DBPool};
use actix_rt::task::spawn_blocking;
use chrono::{DateTime, Utc};

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct ClientMessage {
    pub sender: String,
    pub recipient: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub enum ChatCommand {
    Connect {
        login: String,
        addr: Recipient<ClientMessage>,
    },
    Disconnect {
        login: String,
    },
    SendMessage {
        sender: String,
        recipient: String,
        body: String,
    },
}

pub struct ChatServer {
    sessions: HashMap<String, Recipient<ClientMessage>>,
    db_pool: r2d2::Pool<ConnectionManager<PgConnection>>, // Пул Diesel
}

impl ChatServer {
    pub fn new(db_pool: r2d2::Pool<ConnectionManager<PgConnection>>) -> ChatServer {
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

    fn handle(&mut self, msg: ChatCommand, _: &mut Context<Self>) -> Self::Result {
        match msg {
            ChatCommand::Connect { login, addr } => {
                self.sessions.insert(login, addr);
            }
            ChatCommand::Disconnect { login } => {
                self.sessions.remove(&login);
            }
            ChatCommand::SendMessage {
                sender,
                recipient,
                body,
            } => {
                let pool = self.db_pool.clone();
                let maybe_recipient = self.sessions.get(&recipient).cloned();
                let message = ClientMessage {
                    sender: sender.clone(),
                    recipient: recipient.clone(),
                    body: body.clone(),
                    created_at: Utc::now().to_rfc3339(),
                };

                // Отправка сообщения пользователю, если он онлайн
                if let Some(addr) = maybe_recipient {
                    let _ = addr.do_send(message.clone());
                }

                spawn_blocking(move || {
                    // Сохранение сообщения в базу данных
                    let query = r#"
                        INSERT INTO messages (sender_login, recipient_login, body)
                        VALUES ($1, $2, $3)
                    "#;

                    // Выполняем запрос с привязкой параметров
                    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
                    diesel::sql_query(query)
                        .bind::<diesel::sql_types::Text, _>(sender)  // Sender
                        .bind::<diesel::sql_types::Text, _>(recipient)  // Recipient
                        .bind::<diesel::sql_types::Text, _>(body)  // Body
                        .execute(conn)
                        .expect("Error saving message to DB");
                });
            }
        }
    }
}

pub struct ChatSession {
    pub login: String,
    pub addr: Addr<ChatServer>,
}

impl Actor for ChatSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.addr.do_send(ChatCommand::Connect {
            login: self.login.clone(),
            addr: ctx.address().recipient(),
        });

        ctx.run_interval(std::time::Duration::from_secs(30), |_, ctx| {
            ctx.ping(b"keep-alive");
        });
    }
    

    fn stopped(&mut self, _: &mut Self::Context) {
        self.addr.do_send(ChatCommand::Disconnect {
            login: self.login.clone(),
        });
    }
}

impl StreamHandler<Result<ws::Message, ProtocolError>> for ChatSession {
    fn handle(&mut self, msg: Result<ws::Message, ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<ClientMessage>(&text) {
                    self.addr.do_send(ChatCommand::SendMessage {
                        sender: parsed.sender,
                        recipient: parsed.recipient,
                        body: parsed.body,
                    });
                }
            }
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                // можно логировать
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


impl Handler<ClientMessage> for ChatSession {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, ctx: &mut Self::Context) {
        if let Ok(text) = serde_json::to_string(&msg) {
            ctx.text(text);
        }
    }
}

// === HTTP entrypoint для WS ===
pub async fn chat_ws(
    req: HttpRequest,
    stream: web::Payload,
    login: web::Path<String>,
    srv: web::Data<Addr<ChatServer>>,
) -> Result<HttpResponse, Error> {
    let session = ChatSession {
        login: login.into_inner(),
        addr: srv.get_ref().clone(),
    };
    ws::start(session, &req, stream)
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct MessageDto {
    #[sql_type = "Text"]
    pub sender: String,
    #[sql_type = "Text"]
    pub recipient: String,
    #[sql_type = "Text"]
    pub body: String,
    #[sql_type = "Timestamptz"]
    pub created_at: chrono::NaiveDateTime,
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
  let token = match req.headers().get(header::AUTHORIZATION) {
        Some(header_value) => {
            let header_str = header_value.to_str().unwrap_or("");
            if header_str.starts_with("Bearer ") {
                Some(&header_str[7..])
            } else {
                None
            }
        }
        None => None,
    };

    let claims = match token.and_then(|t| verify_jwt(t)) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let my_login = claims.sub;
    let other_login = query.companion.clone();

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
       SELECT sender_login as sender, recipient_login as recipient, body, created_at
        FROM messages
        WHERE (sender_login = $1 AND recipient_login = $2)
           OR (sender_login = $2 AND recipient_login = $1)
        ORDER BY created_at ASC
    "#;

    let messages = match diesel::sql_query(query)
        .bind::<Text, _>(&my_login)
        .bind::<Text, _>(&other_login)
        .load::<MessageDto>(conn)
    {
        Ok(results) => results,
        Err(err) => {
            eprintln!("DB error: {:?}", err);
            return HttpResponse::InternalServerError().body("Error fetching messages");
        }
    };

    HttpResponse::Ok().json(messages)
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct DialogDto {
    #[sql_type = "Text"]
    pub companion: String,
    #[sql_type = "Text"]
    pub last_message: String,
    #[sql_type = "Timestamptz"]
    pub last_message_time: DateTime<Utc>,
}

#[get("/dialogs")]
async fn get_my_dialogs(
    pool: web::Data<DBPool>,
    req: HttpRequest,
) -> HttpResponse {
    let token = match req.headers().get(header::AUTHORIZATION) {
        Some(header_value) => {
            let header_str = header_value.to_str().unwrap_or("");
            if header_str.starts_with("Bearer ") {
                Some(&header_str[7..])
            } else {
                None
            }
        }
        None => None,
    };

    let claims = match token.and_then(|t| verify_jwt(t)) {
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

    HttpResponse::Ok().json(dialogs)
}
