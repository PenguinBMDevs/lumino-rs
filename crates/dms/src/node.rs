//! DMS 节点数据结构
//!
//! 该模块已拆分为以下子模块：
//! - `types`: `DmsNode` trait 和常量定义
//! - `data`: 原始二进制数据节点 (`DmsDataNode`)
//! - `composite`: 复合节点 (`DmsCompositeNode`)
//! - `string_and_number`: 字符串和数值节点

pub mod composite;
pub mod data;
pub mod string_and_number;
pub mod types;

pub use composite::DmsCompositeNode;
pub use data::DmsDataNode;
pub use string_and_number::{DmsAnsiStringNode, DmsFloatNode, DmsIntegerNode};
pub use types::{DmsLeafDataProvider, DmsNode, create_node};
