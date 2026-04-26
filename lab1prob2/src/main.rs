use serde::Deserialize;
use std::collections::VecDeque;
use std::env;
use std::fs::File;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
struct GridCell {
    x: usize,
    y: usize,
}

#[derive(Debug, Deserialize)]
struct Wall {
    #[serde(rename = "type")]
    _cell_type: String,
    x: usize,
    y: usize,
}

#[derive(Debug, Deserialize)]
struct Labyrinth {
    width: usize,
    height: usize,
    start: GridCell,
    goal: GridCell,
    grid: Vec<Wall>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Not enough arguments.");
        return;
    }

    let file = File::open(&args[1]).expect("Unable to open file");
    let labyrinth: Labyrinth = serde_json::from_reader(file).expect("Unable to parse JSON");

    println!("Loaded Labyrinth: {}x{}", labyrinth.width, labyrinth.height);

    let mut is_wall = vec![vec![false; labyrinth.width]; labyrinth.height];
    for wall in &labyrinth.grid {
        is_wall[wall.y][wall.x] = true;
    }

    let mut queue: VecDeque<GridCell> = VecDeque::new();
    let mut visited = vec![vec![false; labyrinth.width]; labyrinth.height];
    let mut came_from = vec![vec![None; labyrinth.width]; labyrinth.height];

    queue.push_back(labyrinth.start);
    visited[labyrinth.start.y][labyrinth.start.x] = true;

    let directions: [(isize, isize); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
    let mut found = false;

    while let Some(current_cell) = queue.pop_front() {
        if current_cell.x == labyrinth.goal.x && current_cell.y == labyrinth.goal.y {
            found = true;
            break;
        }

        for (dx, dy) in &directions {
            let new_x = current_cell.x as isize + dx;
            let new_y = current_cell.y as isize + dy;

            if new_x >= 0
                && new_x < labyrinth.width as isize
                && new_y >= 0
                && new_y < labyrinth.height as isize
            {
                let nx = new_x as usize;
                let ny = new_y as usize;

                if !visited[ny][nx] && !is_wall[ny][nx] {
                    visited[ny][nx] = true;
                    came_from[ny][nx] = Some(current_cell);
                    queue.push_back(GridCell { x: nx, y: ny });
                }
            }
        }
    }

    if found {
        let mut path = Vec::new();
        let mut curr = labyrinth.goal;

        while curr != labyrinth.start {
            path.push(curr);
            curr = came_from[curr.y][curr.x].unwrap();
        }
        path.push(labyrinth.start);
        path.reverse();

        println!("Success! Path found with {} steps.", path.len());
        println!("Final path: {:?}", path);
    } else {
        println!("No valid path to the goal could be found.");
    }
}