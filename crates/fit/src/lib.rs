//! Presentation, fitting: dots into cells.
//!
//! `exact` turns a canvas into a grid for one rung of the dot ladder.
//! `thrift` simplifies a canvas's colours first, so the grid costs less to send.
pub mod exact;
pub mod thrift;
