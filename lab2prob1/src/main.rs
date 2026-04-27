use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub const PROTOCOL_VERSION: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub command: Command,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Command {
    Hello,
    Login,
    Error,
    Ready,
    Challenge,
    Practice,
    StartMatch,
    StartTurn,
    Move,
    Shoot,
    EndMatch,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub heroes: Vec<PlayerHeroSpawn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerHeroSpawn {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroTypeConfig {
    pub shoot_cooldown: i32,
    pub projectile_ttl: i32,
    pub projectile_speed: i32,
    pub max_hp: i32,
    pub projectile_damage: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub width: i32,
    pub height: i32,
    pub turns: i32,
    pub vision_range: i32,
    pub seed: u32,
    pub players: Vec<Player>,
    pub hero_types: HashMap<String, HeroTypeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hero {
    pub id: i32,
    pub owner_id: i32,
    #[serde(rename = "type")]
    pub type_: String,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub cooldown: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projectile {
    pub owner_id: i32,
    #[serde(rename = "type")]
    pub type_: String,
    pub origin_x: i32,
    pub origin_y: i32,
    pub x: i32,
    pub y: i32,
    pub ttl: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wall {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub heroes: Vec<Hero>,
    #[serde(default)]
    pub projectiles: Vec<Projectile>,
    #[serde(default)]
    pub walls: Vec<Wall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartMatchArgs {
    pub config: GameConfig,
    pub state: GameState,
    pub match_id: String,
    pub your_player_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTurnArgs {
    pub turn: i32,
    pub state: GameState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorArgs {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub fatal: bool,
}

async fn send_command<
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
>(
    write: &mut S,
    msg: WebSocketMessage,
) -> anyhow::Result<()> {
    let msg_deserialized = serde_json::to_string(&msg).context("serialize message")?;
    write
        .send(Message::Text(msg_deserialized.into()))
        .await
        .context("send message")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let url = "wss://bitdefenders.cvjd.me/ws";
    let (ws, _) = connect_async(url).await.unwrap();
    let (mut write, mut read) = ws.split();

    println!("Connected to server");

    let mut my_player_id: Option<i32> = None;
    let mut map_width: i32 = 0;
    let mut map_height: i32 = 0;

    while let Some(msg) = read.next().await {
        let msg = msg.unwrap();

        let text = match msg {
            Message::Text(text) => text,
            Message::Ping(payload) => {
                write.send(Message::Pong(payload)).await.unwrap();
                continue;
            }
            Message::Pong(_) => {
                println!("pong");
                continue;
            }
            Message::Binary(_) => {
                println!("Warning: binary message ignored");
                continue;
            }
            Message::Close(frame) => {
                println!("Connection closed: {frame:?}");
                break;
            }
            Message::Frame(_) => continue,
        };

        let message: WebSocketMessage = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                println!("Error parsing message: {e}");
                continue;
            }
        };

        match message.command {
            Command::Hello => {
                println!("Received HELLO. Sending LOGIN for Bianca...");
                send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Login,
                        args: json!({
                            "version": PROTOCOL_VERSION,
                            "name": "Bianca"
                        }),
                    },
                )
                .await
                .unwrap();
            }
            
            Command::Login => panic!("Server should not send LOGIN!"),
            
            Command::Ready => {
                println!("Authentication successful. Sending PRACTICE command...");
                send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Practice,
                        args: json!({}),
                    },
                )
                .await
                .unwrap();
            }
            
            Command::StartMatch => {
                println!("MATCH STARTING!");
                let match_data: StartMatchArgs = serde_json::from_value(message.args).unwrap();
                
                println!("Match generated successfully! Match ID: {}", match_data.match_id);
                
                my_player_id = Some(match_data.your_player_id);
                map_width = match_data.config.width;
                map_height = match_data.config.height;
            }

            Command::StartTurn => {
                let turn_data: StartTurnArgs = serde_json::from_value(message.args).unwrap();
                println!("--- Turn {} ---", turn_data.turn);

                if let Some(player_id) = my_player_id {
                    let my_heroes: Vec<_> = turn_data.state.heroes.iter().filter(|h| h.owner_id == player_id).collect();
                    let enemy_heroes: Vec<_> = turn_data.state.heroes.iter().filter(|h| h.owner_id != player_id).collect();

                    for hero in my_heroes {
                        let mut closest_enemy = None;
                        let mut min_dist = i32::MAX;
                        
                        for enemy in &enemy_heroes {
                            let dist = (hero.x - enemy.x).abs().max((hero.y - enemy.y).abs());
                            if dist < min_dist {
                                min_dist = dist;
                                closest_enemy = Some(enemy);
                            }
                        }

                        if hero.cooldown == 0 && closest_enemy.is_some() {
                            let target = closest_enemy.unwrap();
                            println!("Hero {} SHOOTS at enemy at {}, {}", hero.id, target.x, target.y);
                            
                            send_command(
                                &mut write,
                                WebSocketMessage {
                                    command: Command::Shoot,
                                    args: json!({
                                        "hero_id": hero.id,
                                        "x": target.x,
                                        "y": target.y
                                    }),
                                },
                            ).await.unwrap();
                            continue;
                        }

                        let possible_moves = [
                            (hero.x, hero.y + 3),
                            (hero.x + 3, hero.y + 3),
                            (hero.x - 3, hero.y + 3),
                            (hero.x + 3, hero.y),
                            (hero.x - 3, hero.y),
                            (hero.x + 3, hero.y - 3),
                            (hero.x - 3, hero.y - 3),
                            (hero.x, hero.y - 3),
                        ];

                        let mut best_move = None;
                        let mut best_dist = i32::MAX;

                        for &(tx, ty) in possible_moves.iter() {
                            let in_bounds = tx >= 1 && tx < map_width - 1 && ty >= 1 && ty < map_height - 1;
                            
                            if in_bounds {
                                let hit_wall = turn_data.state.walls.iter().any(|w| w.x == tx && w.y == ty);
                                
                                if !hit_wall {
                                    if let Some(target) = closest_enemy {
                                        let dist_to_target = (tx - target.x).abs().max((ty - target.y).abs());
                                        if dist_to_target < best_dist {
                                            best_dist = dist_to_target;
                                            best_move = Some((tx, ty));
                                        }
                                    } else {
                                        best_move = Some((tx, ty));
                                        break; 
                                    }
                                }
                            }
                        }

                        if let Some((target_x, target_y)) = best_move {
                            println!("Hero {} moves to {},{}", hero.id, target_x, target_y);
                            send_command(
                                &mut write,
                                WebSocketMessage {
                                    command: Command::Move,
                                    args: json!({
                                        "hero_id": hero.id,
                                        "x": target_x,
                                        "y": target_y
                                    }),
                                },
                            ).await.unwrap();
                        } else {
                            println!("Hero {} is blocked and waiting.", hero.id);
                        }
                    }
                }
            }

            Command::EndMatch => {
                println!("MATCH ENDED!");
                println!("Result: {:?}", message.args);
                break;
            }

            Command::Error => {
                let err_data: Result<ErrorArgs, _> = serde_json::from_value(message.args.clone());
                if let Ok(err) = err_data {
                    println!("ERROR [{}]: {}", err.code, err.message);
                    if err.fatal {
                        break;
                    }
                } else {
                    println!("Unknown ERROR: {:?}", message.args);
                    break;
                }
            }
            
            _ => {}
        }
    }
}