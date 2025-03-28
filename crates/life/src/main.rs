use std::usize;

// enum Direction {
//   UpperLeft = -9,
//   UpperMid = -8,
//   UpperRight = -7,
//   MidLeft = -1,
//   MidRight = 1,
//   BottomLeft = 7,
//   BottomMid = 8,
//   BottomRight = 9,
// }
// const DIRECTIONS: [Direction; 8] = [
//   Direction::UpperLeft,
//   Direction::UpperMid,
//   Direction::UpperRight,
//   Direction::MidLeft,
//   Direction::MidRight,
//   Direction::BottomLeft,
//   Direction::BottomMid,
//   Direction::BottomRight,
// ];
const DIRECTIONS: [i32; 8] = [-9, -8, -7, -1, 1, 7, 8, 9];


#[derive(Clone, Debug)]
struct Pos(usize, usize);
impl Pos {
  pub const EMPTY: Self = Self(usize::MAX, usize::MAX);
}

#[derive(Clone, Debug)]
struct Cell {
  pos: Pos,
  is_alive: bool,
}
impl Cell {
  pub fn new(pos: Pos, is_alive: bool) -> Self {
    return Self { pos, is_alive };
  }

  pub fn die(&mut self) {
    self.is_alive = false;
  }
  pub fn born(&mut self) {
    self.is_alive = true;
  }

  pub fn set_pos(&mut self, pos: Pos) {
    self.pos = pos;
  }
}

#[derive(Debug)]
struct Chunk {
  pos: Pos,
  cells: Vec<Cell>,
}

#[derive(Debug)]
struct Universe {
  chunks: Vec<Chunk>,
  generation: usize,
}
impl Universe {
  pub fn new() -> Self {
    return Self {
      chunks: Vec::new(),
      generation: 0,
    };
  }

  pub fn big_bang(&mut self) {
    loop {
      for chunk in self.chunks.iter() {
        let snapshot: Vec<Cell> = chunk.cells.clone();

        for (cell_idx, cell) in chunk.cells.iter().enumerate() {
          dbg!("{}\n{}", &self.neighbour_count(0, cell_idx), &cell);
        }
      }
    }
  }

  pub fn neighbour_count(&self, chunk_idx: usize, cell_idx: usize) -> usize {
    let chunk: &Chunk = &self.chunks[chunk_idx];
    let mut alive_counter: usize = 0;

    for direction in DIRECTIONS {
      let idx = cell_idx as i32 + direction;
      // dbg!("{:?}", idx);

      if idx > 0 && chunk.cells[idx as usize].is_alive {
        alive_counter += 1;
      };
    }

    return alive_counter;
  }
}


fn main() {
  let mut universe: Universe = Universe::new();
  let mut chunk: Chunk = Chunk {
    pos: Pos(0, 0),
    cells: vec![Cell::new(Pos::EMPTY, false); 64],
  };
  for y in 0..8 {
    for x in 0..8 {
      if let Some(cell) = chunk.cells.get_mut(y * 8 + x) {
        cell.set_pos(Pos(x, y));
      };
    }
  }
  chunk.cells.get_mut(2 * 8 + 2).unwrap().born();
  chunk.cells.get_mut(3 * 8 + 2).unwrap().born();
  chunk.cells.get_mut(4 * 8 + 2).unwrap().born();
  dbg!("{}", &chunk);
  universe.chunks.push(chunk);


  universe.big_bang();
}
