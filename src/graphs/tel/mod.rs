mod compact_triple;
mod triple128;
mod triple64;
mod graph_writer;
mod string_collector;
mod graph;
mod iter;
mod triple;

use self::triple64::*;
use self::triple128::*;
// pub use self::graph::Graph;

pub type Graph64 = graph::Graph<Triple64SPO, Triple64OPS>;
pub type Graph128 = graph::Graph<Triple128SPO, Triple128OPS>;
pub type GraphCreator<A, B> = graph_writer::GraphWriter<A, B>;
