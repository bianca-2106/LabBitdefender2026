use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use tokio_tungstenite::{connect_async, tungstenite::Message};
extern crate bresenham;
use bresenham::Bresenham;

pub const PROTOCOL_VERSION: i32 = 1;
const INT_MAX: i32 = 2147483647;
const INT_MIN: i32 = -2147483648;

macro_rules! MAX {
    ($a:expr, $b:expr) => { if $a > $b { $a } else { $b } };
}
macro_rules! MIN {
    ($a:expr, $b:expr) => { if $a < $b { $a } else { $b } };
}
macro_rules! ABS {
    ($a:expr) => { if $a < 0 { -$a } else { $a } };
}
macro_rules! IDX {
    ($x:expr, $y:expr, $width:expr) => { (($y) * ($width) + ($x)) as usize };
}

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

fn bresenham_line(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut points: Vec<(i32, i32)> = Vec::new();
    let dx: i32 = ABS!(x1 - x0);
    let dy: i32 = -ABS!(y1 - y0);
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err: i32 = dx + dy;
    let mut x: i32 = x0;
    let mut y: i32 = y0;
    
    loop {
        points.push((x, y));
        if x == x1 && y == y1 { 
            break; 
        }
        let e2: i32 = 2 * err;
        if e2 >= dy { 
            err += dy; 
            x += sx; 
        }
        if e2 <= dx { 
            err += dx; 
            y += sy; 
        }
    }
    return points;
}

fn bfs_next_step(start_x: i32, start_y: i32, target_x: i32, target_y: i32, width: i32, height: i32, walls: &Vec<(i32, i32)>) -> Option<(i32, i32)> {
    let grid_size: usize = (width * height) as usize;
    let mut visited: Vec<bool> = vec![false; grid_size];
    let mut parent_x: Vec<i32> = vec![-1; grid_size];
    let mut parent_y: Vec<i32> = vec![-1; grid_size];

    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
    
    queue.push_back((start_x, start_y));
    visited[IDX!(start_x, start_y, width)] = true;
    parent_x[IDX!(start_x, start_y, width)] = start_x;
    parent_y[IDX!(start_x, start_y, width)] = start_y;
    
    let dx: [i32; 8] = [3, 0, 0, -3, 3, 3, -3, -3];
    let dy: [i32; 8] = [0, -3, 3, 0, 3, -3, 3, -3];

    while !queue.is_empty() {
        let curr = queue.pop_front().unwrap();
        let cx = curr.0;
        let cy = curr.1;

        if ABS!(cx - target_x) <= 3 && ABS!(cy - target_y) <= 3 {
            let mut path_x = cx;
            let mut path_y = cy;
            
            while parent_x[IDX!(path_x, path_y, width)] != start_x || parent_y[IDX!(path_x, path_y, width)] != start_y {
                let px = parent_x[IDX!(path_x, path_y, width)];
                let py = parent_y[IDX!(path_x, path_y, width)];
                path_x = px;
                path_y = py;
            }
            return Some((path_x, path_y));
        }
        
        for i in 0..8 {
            let nx = cx + dx[i];
            let ny = cy + dy[i];
            
            let in_bounds = nx >= 1 && nx < width - 1 && ny >= 1 && ny < height - 1;
            if in_bounds {
                let idx = IDX!(nx, ny, width);
                if !visited[idx] {
                    let mut is_wall = false;
                    for w in 0..walls.len() {
                        if walls[w].0 == nx && walls[w].1 == ny {
                            is_wall = true;
                            break;
                        }
                    }
                    
                    if !is_wall {
                        visited[idx] = true;
                        parent_x[idx] = cx;
                        parent_y[idx] = cy;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
    }
    return None;
}

fn get_user_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    return input.trim().to_string();
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
    return Ok(());
}

#[tokio::main]
async fn main() {
    let url = "wss://bitdefenders.cvjd.me/ws";
    let (ws, _) = connect_async(url).await.unwrap();
    let (mut write, mut read) = ws.split();

    println!("Connected to server");

    let mut my_player_id: i32 = -1;
    let mut map_width: i32 = 0;
    let mut map_height: i32 = 0;
    let mut walls_cache: Vec<(i32, i32)> = Vec::new();
    let mut last_known_enemies: HashMap<i32, (i32, i32)> = HashMap::new();
    
    let mut hero_history_1: HashMap<i32, (i32, i32)> = HashMap::new();
    let mut hero_history_2: HashMap<i32, (i32, i32)> = HashMap::new();

    while let Some(msg_result) = read.next().await {
        let msg = msg_result.unwrap();

        let text = match msg {
            Message::Text(text) => text,
            Message::Ping(payload) => {
                write.send(Message::Pong(payload)).await.unwrap();
                continue;
            }
            Message::Pong(_) => continue,
            Message::Binary(_) | Message::Frame(_) => continue,
            Message::Close(frame) => {
                println!("Connection closed: {frame:?}");
                break;
            }
        };

        let message: WebSocketMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                println!("Error parsing message: {e}");
                continue;
            }
        };

        match message.command {
            Command::Hello => {
                send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Login,
                        args: json!({
                            "version": PROTOCOL_VERSION,
                            "name": "Bianca"
                        }),
                    },
                ).await.unwrap();
            }
            
            Command::Login => panic!("Server should not send LOGIN!"),
            
            Command::Ready => {
                println!("\n--- MATCH SETUP ---");
                let mode = get_user_input("Choose mode - (P)ractice or (C)hallenge? ");
                
                if mode.to_uppercase().starts_with('C') {
                    send_command(
                        &mut write,
                        WebSocketMessage {
                            command: Command::Challenge,
                            args: json!({
                                "ranked": false
                            }),
                        },
                    ).await.unwrap();
                } else {
                    let id_input = get_user_input("Choose Player ID - (0) Top, (1) Bottom, or (Enter) Any: ");
                    let mut args = json!({});
                    
                    if id_input == "0" {
                        args["my_id"] = json!(0);
                    } else if id_input == "1" {
                        args["my_id"] = json!(1);
                    }
                    
                    send_command(
                        &mut write,
                        WebSocketMessage {
                            command: Command::Practice,
                            args,
                        },
                    ).await.unwrap();
                }
            }
            
            Command::StartMatch => {
                let match_data: StartMatchArgs = serde_json::from_value(message.args).unwrap();
                
                println!("Match ID: {}", match_data.match_id);
                
                my_player_id = match_data.your_player_id;
                map_width = match_data.config.width;
                map_height = match_data.config.height;
                
                walls_cache.clear();
                for i in 0..match_data.state.walls.len() {
                    walls_cache.push((match_data.state.walls[i].x, match_data.state.walls[i].y));
                }
            }

            Command::StartTurn => {
                let turn_data: StartTurnArgs = serde_json::from_value(message.args).unwrap();

                if my_player_id != -1 {
                    let mut my_heroes: Vec<Hero> = Vec::new();
                    let mut enemy_heroes: Vec<Hero> = Vec::new();
                    
                    for i in 0..turn_data.state.heroes.len() {
                        let h = &turn_data.state.heroes[i];
                        if h.owner_id == my_player_id {
                            my_heroes.push(h.clone());
                        } else {
                            enemy_heroes.push(h.clone());
                            last_known_enemies.insert(h.id, (h.x, h.y));
                        }
                    }
                    
                    let mut primary_target: Option<(i32, i32)> = None;
                    
                    if my_heroes.len() > 0 {
                        let first_hero = &my_heroes[0];
                        let mut min_dist: i32 = INT_MAX;
                        
                        for (_id, pos) in &last_known_enemies {
                            let dist_x = ABS!(first_hero.x - pos.0);
                            let dist_y = ABS!(first_hero.y - pos.1);
                            let dist = MAX!(dist_x, dist_y);
                            
                            if dist < min_dist {
                                min_dist = dist;
                                primary_target = Some(*pos);
                            }
                        }
                    }

                    let mut claimed_tiles: HashSet<(i32, i32)> = HashSet::new();

                    for i in 0..my_heroes.len() {
                        let hero = &my_heroes[i];
                        let mut acted: bool = false;
                        let comment: Option<&str> = None;

                        if let Some(target) = primary_target {
                            let ex = target.0;
                            let ey = target.1;

                            if hero.cooldown == 0 {
                                let line = bresenham_line(hero.x, hero.y, ex, ey);
                                let mut hit_wall = false;
                                
                                for p in 0..line.len() {
                                    if walls_cache.contains(&line[p]) {
                                        hit_wall = true;
                                        break;
                                    }
                                }

                                if !hit_wall {
                                    send_command(
                                        &mut write,
                                        WebSocketMessage {
                                            command: Command::Shoot,
                                            args: json!({
                                                "hero_id": hero.id,
                                                "x": ex,
                                                "y": ey,
                                                "comment": comment
                                            }),
                                        },
                                    ).await.unwrap();
                                    acted = true;
                                }
                            }

                            if !acted {
                                let ideal_distance = if hero.cooldown == 0 { 8 } else { 12 };
                                
                                let possible_moves: [(i32, i32); 8] = [
                                    (hero.x, hero.y + 3), (hero.x + 3, hero.y + 3), (hero.x - 3, hero.y + 3),
                                    (hero.x + 3, hero.y), (hero.x - 3, hero.y),
                                    (hero.x + 3, hero.y - 3), (hero.x - 3, hero.y - 3), (hero.x, hero.y - 3),
                                ];

                                let mut best_move: Option<(i32, i32)> = None;
                                let mut best_score: i32 = INT_MIN;

                                for m in 0..8 {
                                    let nx = possible_moves[m].0;
                                    let ny = possible_moves[m].1;
                                    
                                    let in_bounds = nx >= 1 && nx < map_width - 1 && ny >= 1 && ny < map_height - 1;
                                    
                                    if in_bounds && !claimed_tiles.contains(&(nx, ny)) {
                                        let path_to_move = bresenham_line(hero.x, hero.y, nx, ny);
                                        let mut path_blocked = false;
                                        for p in 0..path_to_move.len() {
                                            if walls_cache.contains(&path_to_move[p]) {
                                                path_blocked = true;
                                                break;
                                            }
                                        }
                                        
                                        if !path_blocked {
                                            let dist_x = ABS!(nx - ex);
                                            let dist_y = ABS!(ny - ey);
                                            let dist = MAX!(dist_x, dist_y);
                                            
                                            let mut score = -ABS!(dist - ideal_distance);

                                            let dist_to_edge_x = MIN!(nx, map_width - 1 - nx);
                                            let dist_to_edge_y = MIN!(ny, map_height - 1 - ny);
                                            let min_dist_to_edge = MIN!(dist_to_edge_x, dist_to_edge_y);
                                            
                                            if min_dist_to_edge < 12 {
                                                score -= (12 - min_dist_to_edge) * 80; 
                                            }
                                            
                                            let h1 = hero_history_1.get(&hero.id);
                                            let h2 = hero_history_2.get(&hero.id);

                                            if let Some(&last_pos) = h1 {
                                                if last_pos == (nx, ny) {
                                                    score -= 50;
                                                }
                                            }

                                            if let Some(&last_last_pos) = h2 {
                                                if last_last_pos == (nx, ny) {
                                                    score -= 30;
                                                }
                                            }

                                            if score > best_score {
                                                best_score = score;
                                                best_move = Some((nx, ny));
                                            }
                                        }
                                    }
                                }

                                if let Some(bm) = best_move {
                                    if let Some(&old_move) = hero_history_1.get(&hero.id) {
                                        hero_history_2.insert(hero.id, old_move);
                                    }
                                    hero_history_1.insert(hero.id, bm);
                                    claimed_tiles.insert(bm);

                                    send_command(
                                        &mut write,
                                        WebSocketMessage {
                                            command: Command::Move,
                                            args: json!({
                                                "hero_id": hero.id,
                                                "x": bm.0,
                                                "y": bm.1,
                                                "comment": comment
                                            }),
                                        },
                                    ).await.unwrap();
                                    acted = true;
                                }
                            }
                        } else {
                            if !acted {
                                let path = bfs_next_step(hero.x, hero.y, map_width / 2, map_height / 2, map_width, map_height, &walls_cache);
                                if let Some(nxt) = path {
                                    claimed_tiles.insert(nxt);
                                    send_command(
                                        &mut write,
                                        WebSocketMessage {
                                            command: Command::Move,
                                            args: json!({
                                                "hero_id": hero.id,
                                                "x": nxt.0,
                                                "y": nxt.1,
                                                "comment": comment
                                            }),
                                        },
                                    ).await.unwrap();
                                }
                            }
                        }
                    }
                }
            }

            Command::EndMatch => {
                let reason = message.args["reason"].as_str().unwrap_or("unknown");
                
                if let Some(winner) = message.args["winner"].as_str() {
                    println!("\nMATCH ENDED! Reason: {}. Winner: {}", reason, winner);
                } else {
                    println!("\nMATCH ENDED! Reason: {}", reason);
                }
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