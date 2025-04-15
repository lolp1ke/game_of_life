use std::{
  collections::HashMap,
  io::{self, Write},
  time,
};

use anyhow::Result;
use crossterm::{
  ExecutableCommand, QueueableCommand, cursor, event, style, terminal,
};
use futures::{FutureExt, StreamExt, future::Fuse, select, stream::Next};
use futures_timer::Delay;


const CHUNK_SIZE: usize = 32;
const CHUNK_SIZE_SQR: usize = CHUNK_SIZE * CHUNK_SIZE;
const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;
const CHUNK_SIZE_SQR_I32: i32 = CHUNK_SIZE_SQR as i32;

const CHUNKS_TO_DRAW: i32 = 1;

const SPEED: u64 = 200;

const DIRECTIONS: [i32; 8] = [
  -CHUNK_SIZE_I32 - 1,
  -CHUNK_SIZE_I32,
  -CHUNK_SIZE_I32 + 1,
  -1,
  1,
  CHUNK_SIZE_I32 - 1,
  CHUNK_SIZE_I32,
  CHUNK_SIZE_I32 + 1,
];


#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Pos(i32, i32);
impl Pos {
  const EMPTY: Self = Self(i32::MAX, i32::MAX);
}


#[derive(Clone, Debug)]
struct Cell {
  pos: Pos,
  is_alive: bool,
}
// impl Cell {}
#[derive(Clone, Debug)]
struct Chunk {
  pos: Pos,
  cells: [Cell; CHUNK_SIZE_SQR],
}
impl Chunk {
  fn new_dead(pos: Pos) -> Self {
    return Self {
      pos,
      cells: core::array::from_fn(|i: usize| Cell {
        pos: Pos((i % CHUNK_SIZE) as i32, (i / CHUNK_SIZE) as i32),
        is_alive: false,
      }),
    };
  }

  fn is_alive_at(&self, cell_idx: usize) -> bool {
    debug_assert!(cell_idx < CHUNK_SIZE_SQR);
    return self.cells[cell_idx].is_alive;
  }

  fn is_dead(&self) -> bool {
    for cell in self.cells.iter() {
      if cell.is_alive {
        return false;
      };
    }

    return true;
  }

  fn within_viewport(&self, v_pos: Pos) -> bool {
    return (v_pos.0..(v_pos.0 + CHUNKS_TO_DRAW)).contains(&self.pos.0)
      && (v_pos.1..(v_pos.1 + CHUNKS_TO_DRAW)).contains(&self.pos.1);
  }
}
#[derive(Debug)]
struct Game {
  chunks: HashMap<Pos, Chunk>,
  generation: u128,

  v_pos: Pos,
  stdout: io::Stdout,

  auto: bool,
}
impl Game {
  fn new() -> Self {
    let stdout: io::Stdout = io::stdout();
    let mut chunks: HashMap<Pos, Chunk> = HashMap::new();
    for i in 0..CHUNKS_TO_DRAW {
      for j in 0..CHUNKS_TO_DRAW {
        let pos: Pos = Pos(j, i);
        chunks.insert(pos.clone(), Chunk::new_dead(pos));
      }
    }
    // let chunks: HashMap<Pos, Chunk> = RefCell::new(chunks);


    return Self {
      chunks,
      generation: 0,

      v_pos: Pos(0, 0),
      stdout,

      auto: true,
    };
  }


  fn draw_frame(&mut self) -> Result<()> {
    self
      .stdout
      .queue(terminal::Clear(terminal::ClearType::All))?;


    for (chunk_pos, chunk) in self.chunks.iter() {
      if !chunk.within_viewport(self.v_pos.clone()) {
        continue;
      };

      for cell in chunk.cells.iter() {
        let Pos(local_x, local_y) = cell.pos;
        let global_x = local_x + CHUNK_SIZE_I32 * chunk_pos.0;
        let global_y = local_y + CHUNK_SIZE_I32 * chunk_pos.1;
        let (screen_x, screen_y) = (
          global_x - CHUNK_SIZE_I32 * self.v_pos.0,
          global_y - CHUNK_SIZE_I32 * self.v_pos.1,
        );
        let (screen_x, screen_y) = (screen_x as u16, screen_y as u16);

        self
          .stdout
          .queue(cursor::MoveTo(screen_x, screen_y))?
          .queue(if cell.is_alive {
            style::Print("@")
          } else {
            style::Print("*")
          })?;
      }
    }


    return Ok(());
  }
  async fn run(&mut self) -> Result<()> {
    terminal::enable_raw_mode()?;
    self
      .stdout
      .execute(terminal::EnterAlternateScreen)?
      .execute(terminal::Clear(terminal::ClearType::All))?;


    let mut reader: event::EventStream = event::EventStream::new();
    loop {
      let mut delay: Fuse<Delay> =
        futures_timer::Delay::new(time::Duration::from_millis(SPEED)).fuse();
      let mut event: Fuse<Next<'_, event::EventStream>> = reader.next().fuse();

      select! {
        _ = delay => {
          if self.auto {
            self.step()?;
            self.draw_frame()?;

            self.stdout.flush()?;
          };
        }

        _event = event => {
          match _event {
            Some(Ok(event)) => {
              match event {
                event::Event::Key(event::KeyEvent {code, kind, ..}) if kind == event::KeyEventKind::Release => {
                  match code {
                    event::KeyCode::Char(ch) => {
                      match ch {
                        'q' => break,

                        'l' => self.v_pos.0 += 1,
                        'h' => self.v_pos.0 -= 1,
                        'k' => self.v_pos.1 -= 1,
                        'j' => self.v_pos.1 += 1,

                        ' ' => self.auto = !self.auto,

                        _ => {}
                      };
                    }
                    event::KeyCode::Enter if !self.auto => {
                      self.step()?;
                      self.draw_frame()?;

                      self.stdout.flush()?;
                    }

                    _ => {}
                  };
                }

                _ => {}
              };
            }
            _ => {}
          };
        }
      };
    }

    terminal::disable_raw_mode()?;
    self.stdout.execute(terminal::LeaveAlternateScreen)?;
    return Ok(());
  }

  fn step(&mut self) -> Result<()> {
    for (chunk_pos, chunk) in self.chunks.clone().iter() {
      for (cell_idx, _) in chunk.cells.iter().enumerate() {
        if chunk.is_dead() && !chunk.within_viewport(self.v_pos.clone()) {
          continue;
        };
        let neighbours: u32 = self.check_neighbours(&chunk, cell_idx);

        if neighbours == 3 {
          self.get_cell_mut(&chunk_pos, cell_idx).is_alive = true;
        } else if neighbours < 2 || neighbours > 3 {
          self.get_cell_mut(&chunk_pos, cell_idx).is_alive = false;
        };
      }
    }

    self.generation += 1;
    println!("{} - {}:{}", self.generation, self.v_pos.0, self.v_pos.1);
    return Ok(());
  }

  fn get_cell_mut(&mut self, chunk_pos: &Pos, cell_idx: usize) -> &mut Cell {
    debug_assert!((0..CHUNK_SIZE_SQR).contains(&cell_idx));
    if let Some(chunk) = self.chunks.get_mut(chunk_pos) {
      return &mut chunk.cells[cell_idx];
    };

    panic!("Invalid chunk position");
  }

  fn check_neighbours(
    &mut self,
    current_chunk: &Chunk,
    cell_idx: usize,
  ) -> u32 {
    let mut neighbours: u32 = 0;

    for direction in DIRECTIONS {
      match cell_idx {
        // top left corner
        // _ if cell_idx == 0 => {
        //   let pos: Pos = Pos(current_chunk.pos.0 - 1, current_chunk.pos.1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE - 1,
        //       CHUNK_SIZE * 2 - 1,
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0 - 1, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours +=
        //       self.check_adjacent_neighbour(chunk, &[CHUNK_SIZE_SQR - 1]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE_SQR - (CHUNK_SIZE - 1),
        //       CHUNK_SIZE_SQR - (CHUNK_SIZE - 2),
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        // }
        // // top right corner
        // _ if cell_idx == CHUNK_SIZE - 1 => {
        //   let pos: Pos = Pos(current_chunk.pos.0, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE_SQR,
        //       CHUNK_SIZE_SQR - 1,
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0 + 1, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE_SQR - (CHUNK_SIZE - 1),
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0 + 1, current_chunk.pos.1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours +=
        //       self.check_adjacent_neighbour(chunk, &[0, CHUNK_SIZE]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        // }
        // // bottom left corner
        // _ if cell_idx == CHUNK_SIZE_SQR - (CHUNK_SIZE - 1) => {
        //   let pos: Pos = Pos(current_chunk.pos.0 - 1, current_chunk.pos.1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE - 1,
        //       CHUNK_SIZE * 2 - 1,
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0 - 1, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours +=
        //       self.check_adjacent_neighbour(chunk, &[CHUNK_SIZE_SQR - 1]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        //
        //   let pos: Pos = Pos(current_chunk.pos.0, current_chunk.pos.1 + 1);
        //   if let Some(chunk) = self.chunks.get(&pos) {
        //     neighbours += self.check_adjacent_neighbour(chunk, &[
        //       CHUNK_SIZE_SQR - (CHUNK_SIZE - 1),
        //       CHUNK_SIZE_SQR - (CHUNK_SIZE - 2),
        //     ]);
        //   } else {
        //     self.chunks.insert(pos.clone(), Chunk::new_dead(pos));
        //   };
        // }
        _ => {}
      };


      let idx = cell_idx as i32 + direction;
      if idx < 0 || idx >= CHUNK_SIZE_SQR_I32 {
        continue;
      };

      if current_chunk.is_alive_at(idx as usize) {
        neighbours += 1;
      };
    }


    return neighbours;
  }
  fn check_adjacent_neighbour(
    &self,
    chunk: &Chunk,
    cell_idxs: &[usize],
  ) -> u32 {
    let mut neighbours: u32 = 0;
    for &cell_idx in cell_idxs {
      if chunk.is_alive_at(cell_idx) {
        neighbours += 1;
      };
    }

    return neighbours;
  }
}


#[tokio::main]
async fn main() -> Result<()> {
  // let mut hm = HashMap::new();
  let mut universe: Game = Game::new();
  let mut chunk: Chunk = Chunk::new_dead(Pos(0, 0));

  chunk.cells[5 + 1 * CHUNK_SIZE].is_alive = true;
  chunk.cells[6 + 2 * CHUNK_SIZE].is_alive = true;
  chunk.cells[4 + 3 * CHUNK_SIZE].is_alive = true;
  chunk.cells[5 + 3 * CHUNK_SIZE].is_alive = true;
  chunk.cells[6 + 3 * CHUNK_SIZE].is_alive = true;

  universe.chunks.insert(Pos(0, 0), chunk);
  universe.auto = false;
  universe.run().await?;

  let a = universe
    .chunks
    .get(&Pos(0, 0))
    .unwrap()
    .cells
    .iter()
    .filter(|cell| cell.is_alive)
    .collect::<Vec<&Cell>>();
  dbg!(&a);


  return Ok(());
}
