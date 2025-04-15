use std::{
  cell::{RefCell, RefMut},
  collections::{HashMap, VecDeque},
  fmt::Debug,
  io::{self, Write},
  time,
};

use anyhow::Result;
use crossterm::{
  ExecutableCommand, QueueableCommand, cursor, event, style, terminal,
};
use futures::{FutureExt, StreamExt, future::Fuse, select, stream::Next};
use futures_timer::Delay;


const CHUNK_SIZE: usize = 8;
const CHUNK_SIZE_SQR: usize = CHUNK_SIZE * CHUNK_SIZE;
const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;

const CHUNKS_TO_DRAW: i32 = 5;

const SPEED: u64 = 50;

const OFFSETS: [(i32, i32); 8] = [
  (-1, -1),
  (0, -1),
  (1, -1),
  (-1, 0),
  (1, 0),
  (-1, 1),
  (0, 1),
  (1, 1),
];


#[derive(Debug)]
enum Action {
  NewChunkAt { x: i32, y: i32 },

  CheckCellAt { x: i32, y: i32, idx: usize },
  CheckChunkAt { x: i32, y: i32 },

  MoveLeft,
  MoveRight,
  MoveUp,
  MoveDown,

  ChangeMode,
}

#[derive(Debug)]
struct Cell {
  is_alive: [bool; 2],

  x: i32,
  y: i32,
}
#[derive(Debug)]
struct Chunk {
  cells: [Cell; CHUNK_SIZE_SQR],

  x: i32,
  y: i32,
}
impl Chunk {
  fn new(x: i32, y: i32) -> Self {
    return Self {
      cells: std::array::from_fn(|i: usize| {
        let i = i as i32;
        return Cell {
          is_alive: [false, false],
          x: i % CHUNK_SIZE_I32,
          y: i / CHUNK_SIZE_I32,
        };
      }),
      x,
      y,
    };
  }

  fn within_viewport(&self, vx: i32, vy: i32) -> bool {
    return (vx..(vx + CHUNKS_TO_DRAW)).contains(&self.x)
      && (vy..(vy + CHUNKS_TO_DRAW)).contains(&self.y);
  }
}
#[derive(Debug)]
struct Universe {
  chunks: HashMap<(i32, i32), Chunk>,

  actions: RefCell<VecDeque<Action>>,

  auto: bool,
  generation: usize,

  render: RefCell<Box<dyn Render>>,
}
impl Universe {
  fn new() -> Result<Self> {
    return Ok(Self {
      chunks: HashMap::new(),

      actions: RefCell::new(VecDeque::new()),

      auto: false,
      generation: 0,

      render: RefCell::new(Box::new(TermRender::new()?)),
    });
  }

  fn render<'a>(&'a self) -> RefMut<'a, Box<dyn Render>> {
    return self.render.borrow_mut();
  }

  fn step(&mut self) -> Result<()> {
    for (&(x, y), _) in self.chunks.iter() {
      self
        .actions
        .borrow_mut()
        .push_back(Action::CheckChunkAt { x, y });
    }

    self.execute_actions()?;
    self.render().draw_frame(&self.chunks)?;
    self.generation += 1;
    return Ok(());
  }
  fn check_neighbours(&self, cx: i32, cy: i32, cell: &Cell) -> u32 {
    let global_x = cx * CHUNK_SIZE_I32 + cell.x;
    let global_y = cy * CHUNK_SIZE_I32 + cell.y;
    let mut count = 0;
    for &(dx, dy) in OFFSETS.iter() {
      let neighbour_global_x = global_x + dx;
      let neighbour_global_y = global_y + dy;

      let neighbour_cx = if neighbour_global_x >= 0 {
        neighbour_global_x / CHUNK_SIZE_I32
      } else {
        (neighbour_global_x - (CHUNK_SIZE_I32 - 1)) / CHUNK_SIZE_I32
      };
      let neighbour_cy = if neighbour_global_y >= 0 {
        neighbour_global_y / CHUNK_SIZE_I32
      } else {
        (neighbour_global_y - (CHUNK_SIZE_I32 - 1)) / CHUNK_SIZE_I32
      };

      let neigbhour_local_x =
        neighbour_global_x - neighbour_cx * CHUNK_SIZE_I32;
      let neighbour_local_y =
        neighbour_global_y - neighbour_cy * CHUNK_SIZE_I32;

      if let Some(neighbor_chunk) =
        self.chunks.get(&(neighbour_cx, neighbour_cy))
      {
        let n_idx =
          (neighbour_local_y * CHUNK_SIZE_I32 + neigbhour_local_x) as usize;
        if neighbor_chunk.cells[n_idx].is_alive[0] {
          count += 1;
        }
      };
    }
    return count;
  }

  async fn run(&mut self) -> Result<()> {
    let mut reader: event::EventStream = event::EventStream::new();


    self.render().draw_frame(&self.chunks)?;
    loop {
      let mut delay: Fuse<Delay> =
        futures_timer::Delay::new(time::Duration::from_millis(1000 / SPEED))
          .fuse();
      let mut event: Fuse<Next<'_, event::EventStream>> = reader.next().fuse();

      select! {
        _ = delay => {
          if self.auto {
            self.step()?;
          };
        }

        _event = event => {
          match _event {
            Some(Ok(event)) if self.handle_event(event.clone()).unwrap_or_else(|err| {
              dbg!("Err: {}", err);
              return true;
            }) => break,

            Some(Err(err)) => panic!("Err: {}", err),
            _ => {}
          };
        }
      }
    }


    return Ok(());
  }
  fn handle_event(&mut self, event: event::Event) -> Result<bool> {
    match event {
      event::Event::Key(event::KeyEvent { code, kind, .. })
        if kind == event::KeyEventKind::Press =>
      {
        match code {
          event::KeyCode::Char(ch) => {
            match ch {
              'q' => return Ok(true),

              'h' => self.actions.borrow_mut().push_back(Action::MoveLeft),
              'l' => self.actions.borrow_mut().push_back(Action::MoveRight),
              'k' => self.actions.borrow_mut().push_back(Action::MoveUp),
              'j' => self.actions.borrow_mut().push_back(Action::MoveDown),

              'n' => self.step()?,

              ' ' => self.actions.borrow_mut().push_back(Action::ChangeMode),

              _ => {}
            };
          }

          _ => {}
        };
      }

      _ => {}
    };

    return Ok(false);
  }
  fn execute_actions(&mut self) -> Result<()> {
    use Action::*;
    let mut actions = self.actions.borrow_mut();

    while let Some(action) = actions.pop_front() {
      match action {
        NewChunkAt { x, y } => {
          self.chunks.insert((x, y), Chunk::new(x, y));
          actions.push_front(CheckChunkAt { x, y });
        }

        CheckChunkAt { x, y } => {
          if let Some(chunk) = self.chunks.get_mut(&(x, y)) {
            for (cell_idx, _) in chunk.cells.iter_mut().enumerate() {
              actions.push_back(CheckCellAt {
                x,
                y,
                idx: cell_idx,
              });
            }
          } else {
            actions.push_front(Action::NewChunkAt { x, y });
          };
        }
        CheckCellAt { x, y, idx } => {
          if let Some(chunk) = self.chunks.get(&(x, y)) {
            if chunk.cells[idx].is_alive[0] {
              for &(dx, dy) in OFFSETS.iter() {
                if self.chunks.get(&(x + dx, y + dy)).is_none() {
                  actions.push_front(NewChunkAt {
                    x: x + dx,
                    y: y + dy,
                  });
                };
              }
            };

            let neighbours = self.check_neighbours(x, y, &chunk.cells[idx]);
            if neighbours < 2 || neighbours > 3 {
              self.chunks.get_mut(&(x, y)).unwrap().cells[idx].is_alive[1] =
                false;
            } else if neighbours == 3 {
              self.chunks.get_mut(&(x, y)).unwrap().cells[idx].is_alive[1] =
                true;
            } else if neighbours == 2 {
              if self.chunks.get(&(x, y)).unwrap().cells[idx].is_alive[0] {
                self.chunks.get_mut(&(x, y)).unwrap().cells[idx].is_alive[1] =
                  true;
              };
            };
          };
        }

        MoveLeft => self.render().increment_viewport(-1, 0),
        MoveRight => self.render().increment_viewport(1, 0),
        MoveUp => self.render().increment_viewport(0, -1),
        MoveDown => self.render().increment_viewport(0, 1),

        ChangeMode => self.auto = !self.auto,
      };
    }


    for (_, Chunk { cells, .. }) in self.chunks.iter_mut() {
      for cell in cells.iter_mut() {
        cell.is_alive[0] = cell.is_alive[1];
        cell.is_alive[1] = false;
      }
    }
    return Ok(());
  }
}
trait Render: Debug {
  fn draw_frame(&mut self, chunks: &HashMap<(i32, i32), Chunk>) -> Result<()>;
  fn increment_viewport(&mut self, vx: i32, vy: i32);
}
#[derive(Debug)]
struct TermRender {
  stdout: io::Stdout,

  vx: i32,
  vy: i32,
}
impl Drop for TermRender {
  fn drop(&mut self) {
    let _ = self.stdout.execute(terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
  }
}
impl TermRender {
  const ASSETS: [char; 2] = ['@', '*'];

  fn new() -> Result<Self> {
    let mut stdout: io::Stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout
      .execute(terminal::EnterAlternateScreen)?
      .execute(terminal::Clear(terminal::ClearType::All))?;

    return Ok(Self {
      stdout,

      vx: 0,
      vy: 0,
    });
  }
}
impl Render for TermRender {
  fn draw_frame(&mut self, chunks: &HashMap<(i32, i32), Chunk>) -> Result<()> {
    self
      .stdout
      .queue(terminal::Clear(terminal::ClearType::All))?;


    for (&(x, y), chunk) in chunks.iter() {
      if !chunk.within_viewport(self.vx, self.vy) {
        continue;
      };

      for (cell_idx, cell) in chunk.cells.iter().enumerate() {
        let local_x = (cell_idx % CHUNK_SIZE) as i32;
        let local_y = (cell_idx / CHUNK_SIZE) as i32;
        let global_x = local_x + CHUNK_SIZE_I32 * x;
        let global_y = local_y + CHUNK_SIZE_I32 * y;
        let screen_x = (global_x - CHUNK_SIZE_I32 * self.vx) as u16;
        let screen_y = (global_y - CHUNK_SIZE_I32 * self.vy) as u16;


        self
          .stdout
          .queue(cursor::MoveTo(screen_x, screen_y))?
          .queue(style::Print(if cell.is_alive[0] {
            Self::ASSETS[0]
          } else {
            Self::ASSETS[1]
          }))?;
      }
    }


    self.stdout.flush()?;
    return Ok(());
  }

  fn increment_viewport(&mut self, vx: i32, vy: i32) {
    self.vx += vx;
    self.vy += vy;
  }
}


#[tokio::main]
async fn main() -> Result<()> {
  let mut universe = Universe::new()?;

  let mut chunk = Chunk::new(0, 0);
  chunk.cells[1 + 1 * CHUNK_SIZE].is_alive[0] = true;
  chunk.cells[3 + 2 * CHUNK_SIZE].is_alive[0] = true;
  chunk.cells[1 + 3 * CHUNK_SIZE].is_alive[0] = true;
  chunk.cells[2 + 3 * CHUNK_SIZE].is_alive[0] = true;
  chunk.cells[3 + 3 * CHUNK_SIZE].is_alive[0] = true;

  universe.chunks.insert((0, 0), chunk);
  universe.auto = true;
  universe.run().await?;

  return Ok(());
}
