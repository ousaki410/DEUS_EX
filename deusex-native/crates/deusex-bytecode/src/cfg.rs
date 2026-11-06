//! Control-flow graph recovery for decoded UnrealScript functions.
//!
//! Jump/case/iterator offsets are expressed in the same *normalized* byte
//! coordinate system as [`Expr::offset`](crate::Expr::offset). Every target
//! in the functions we validated points at the start of a top-level statement
//! (or at `script_size` for the implicit function end).

use std::collections::{BTreeMap, BTreeSet};

use crate::function::{Expr, ExprKind, Function};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Fallthrough,
    Jump,
    True,
    False,
    Case,
    CaseDefault,
    Iterator,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub target: usize,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Fallthrough,
    Jump(u32),
    JumpIfNot { target: u32 },
    Case { target: Option<u32> },
    Skip { target: u32 },
    Iterator { target: u32 },
    Return,
    Stop,
    None,
}

impl Terminator {
    pub fn targets(&self) -> Vec<u32> {
        match self {
            Terminator::Jump(target) => vec![*target],
            Terminator::JumpIfNot { target }
            | Terminator::Skip { target }
            | Terminator::Iterator { target } => vec![*target],
            Terminator::Case { target: Some(target) } => vec![*target],
            _ => Vec::new(),
        }
    }

    pub fn is_terminator(&self) -> bool {
        !matches!(self, Terminator::Fallthrough | Terminator::None)
    }
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub start: u32,
    pub end: u32,
    /// Indices into `Function::body`.
    pub statements: Vec<usize>,
    pub terminator: Terminator,
    pub successors: Vec<Edge>,
}

#[derive(Debug, Clone)]
pub struct Cfg {
    pub entry: usize,
    pub blocks: Vec<BasicBlock>,
    /// For every top-level statement, the block containing it.
    pub statement_block: Vec<usize>,
    /// Jump targets that did not resolve to a basic-block start.
    pub unresolved_targets: Vec<(u32, EdgeKind)>,
    /// Label offsets from `EX_LabelTable`.
    pub label_offsets: BTreeMap<u32, Vec<i32>>,
}

impl Cfg {
    pub fn block(&self, id: usize) -> Option<&BasicBlock> {
        self.blocks.get(id)
    }

    pub fn block_at_offset(&self, offset: u32) -> Option<&BasicBlock> {
        self.blocks.iter().find(|block| block.start == offset)
    }
}

/// Build a basic-block CFG from a function's top-level statements.
pub fn build_cfg(function: &Function) -> Cfg {
    let body = &function.body;
    if body.is_empty() {
        return Cfg {
            entry: 0,
            blocks: Vec::new(),
            statement_block: Vec::new(),
            unresolved_targets: Vec::new(),
            label_offsets: BTreeMap::new(),
        };
    }

    let statement_offsets: BTreeMap<u32, usize> = body
        .iter()
        .enumerate()
        .map(|(index, expr)| (expr.offset, index))
        .collect();

    let mut leaders = BTreeSet::new();
    leaders.insert(body[0].offset);

    let mut label_offsets: BTreeMap<u32, Vec<i32>> = BTreeMap::new();
    let mut unresolved_targets = Vec::new();

    for (_index, expr) in body.iter().enumerate() {
        if let ExprKind::LabelTable(labels) = &expr.kind {
            for (name, offset) in labels {
                if *name == 0 {
                    continue;
                }
                label_offsets.entry(*offset).or_default().push(*name);
                leaders.insert(*offset);
            }
        }
    }

    for (index, expr) in body.iter().enumerate() {
        let terminator = terminator_of(expr);
        if terminator.is_terminator() {
            if let Some(next) = body.get(index + 1) {
                leaders.insert(next.offset);
            }
        }
        for target in terminator.targets() {
            if statement_offsets.contains_key(&target) || target == function.script_size {
                leaders.insert(target);
            } else {
                unresolved_targets.push((target, target_kind(&terminator)));
            }
        }
    }
    let mut blocks: Vec<BasicBlock> = Vec::new();
    let mut statement_block = vec![0usize; body.len()];
    let mut current = BasicBlock {
        id: 0,
        start: body[0].offset,
        end: 0,
        statements: Vec::new(),
        terminator: Terminator::None,
        successors: Vec::new(),
    };

    for (index, expr) in body.iter().enumerate() {
        if !current.statements.is_empty() && leaders.contains(&expr.offset) {
            current.end = expr.offset;
            finish_block(&mut current, &body, &statement_offsets, &mut statement_block);
            blocks.push(current);
            current = BasicBlock {
                id: blocks.len(),
                start: expr.offset,
                end: 0,
                statements: Vec::new(),
                terminator: Terminator::None,
                successors: Vec::new(),
            };
        }
        current.statements.push(index);
    }
    current.end = function.script_size;
    finish_block(&mut current, &body, &statement_offsets, &mut statement_block);
    blocks.push(current);

    let start_to_block: BTreeMap<u32, usize> = blocks
        .iter()
        .map(|block| (block.start, block.id))
        .collect();

    let fallthrough: Vec<Option<usize>> = (0..blocks.len())
        .map(|id| blocks.get(id + 1).map(|next| next.id))
        .collect();

    for (id, block) in blocks.iter_mut().enumerate() {
        let fallthrough = fallthrough[id];
        let mut successors = Vec::new();
        match block.terminator {
            Terminator::Jump(target) => {
                push_target(&mut successors, &start_to_block, target, EdgeKind::Jump, &mut unresolved_targets);
            }
            Terminator::JumpIfNot { target } => {
                push_target(&mut successors, &start_to_block, target, EdgeKind::False, &mut unresolved_targets);
                if let Some(next) = fallthrough {
                    successors.push(Edge {
                        target: next,
                        kind: EdgeKind::True,
                    });
                }
            }
            Terminator::Case { target } => {
                if let Some(target) = target {
                    push_target(
                        &mut successors,
                        &start_to_block,
                        target,
                        EdgeKind::Case,
                        &mut unresolved_targets,
                    );
                }
                if let Some(next) = fallthrough {
                    successors.push(Edge {
                        target: next,
                        kind: EdgeKind::CaseDefault,
                    });
                }
            }
            Terminator::Skip { target } => {
                push_target(
                    &mut successors,
                    &start_to_block,
                    target,
                    EdgeKind::Skip,
                    &mut unresolved_targets,
                );
                if let Some(next) = fallthrough {
                    successors.push(Edge {
                        target: next,
                        kind: EdgeKind::Fallthrough,
                    });
                }
            }
            Terminator::Iterator { target } => {
                push_target(
                    &mut successors,
                    &start_to_block,
                    target,
                    EdgeKind::Iterator,
                    &mut unresolved_targets,
                );
                if let Some(next) = fallthrough {
                    successors.push(Edge {
                        target: next,
                        kind: EdgeKind::Fallthrough,
                    });
                }
            }
            Terminator::Return | Terminator::Stop => {}
            Terminator::Fallthrough | Terminator::None => {
                if let Some(next) = fallthrough {
                    successors.push(Edge {
                        target: next,
                        kind: EdgeKind::Fallthrough,
                    });
                }
            }
        }
        block.successors = successors;
    }

    let entry = blocks
        .iter()
        .find(|block| block.start == 0)
        .map(|block| block.id)
        .unwrap_or(0);

    Cfg {
        entry,
        blocks,
        statement_block,
        unresolved_targets,
        label_offsets,
    }
}

fn finish_block(
    block: &mut BasicBlock,
    body: &[Expr],
    statement_offsets: &BTreeMap<u32, usize>,
    statement_block: &mut [usize],
) {
    for index in &block.statements {
        statement_block[*index] = block.id;
    }
    if let Some(last) = block.statements.last() {
        block.terminator = terminator_of(&body[*last]);
        block.end = block
            .end
            .max(statement_offsets.get(&body[*last].offset).map(|_| body[*last].offset + body[*last].size).unwrap_or(0));
    }
}

fn terminator_of(expr: &Expr) -> Terminator {
    match &expr.kind {
        ExprKind::Jump(target) => Terminator::Jump(u32::from(*target)),
        ExprKind::JumpIfNot { offset, .. } => Terminator::JumpIfNot {
            target: u32::from(*offset),
        },
        ExprKind::Case { offset, .. } => Terminator::Case {
            target: (*offset != 0xffff).then_some(u32::from(*offset)),
        },
        ExprKind::Skip { offset, .. } => Terminator::Skip {
            target: u32::from(*offset),
        },
        ExprKind::Iterator { offset, .. } => Terminator::Iterator {
            target: u32::from(*offset),
        },
        ExprKind::Return(_) => Terminator::Return,
        ExprKind::Stop => Terminator::Stop,
        _ => Terminator::Fallthrough,
    }
}

fn target_kind(terminator: &Terminator) -> EdgeKind {
    match terminator {
        Terminator::Jump(_) => EdgeKind::Jump,
        Terminator::JumpIfNot { .. } => EdgeKind::False,
        Terminator::Case { .. } => EdgeKind::Case,
        Terminator::Skip { .. } => EdgeKind::Skip,
        Terminator::Iterator { .. } => EdgeKind::Iterator,
        _ => EdgeKind::Fallthrough,
    }
}

fn push_target(
    successors: &mut Vec<Edge>,
    start_to_block: &BTreeMap<u32, usize>,
    target: u32,
    kind: EdgeKind,
    unresolved: &mut Vec<(u32, EdgeKind)>,
) {
    if let Some(id) = start_to_block.get(&target) {
        successors.push(Edge { target: *id, kind });
    } else {
        unresolved.push((target, kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::{Function, Expr};

    fn expr(offset: u32, size: u32, kind: ExprKind) -> Expr {
        Expr {
            offset,
            size,
            raw_offset: offset as usize,
            raw_size: size as usize,
            kind,
        }
    }

    fn function(body: Vec<Expr>) -> Function {
        Function {
            export_ref: 1,
            name: "Test".to_string(),
            outer_ref: 0,
            outer_name: String::new(),
            super_ref: 0,
            next_ref: 0,
            script_text_ref: 0,
            children_ref: 0,
            friendly_name: "Test".to_string(),
            line: 0,
            text_pos: 0,
            script_size: body.last().map(|e| e.offset + e.size).unwrap_or(0),
            script_offset: 0,
            script_raw_size: 0,
            script: Vec::new(),
            body,
            native_index: 0,
            precedence: 0,
            function_flags: 0,
            replication_offset: None,
            export_end: 0,
            reader_end: 0,
        }
    }

    #[test]
    fn builds_if_else_blocks() {
        let body = vec![
            expr(0, 5, ExprKind::JumpIfNot {
                offset: 10,
                condition: Box::new(expr(1, 4, ExprKind::True)),
            }),
            expr(5, 5, ExprKind::Return(Box::new(expr(6, 4, ExprKind::Nothing)))),
            expr(10, 5, ExprKind::Return(Box::new(expr(11, 4, ExprKind::Nothing)))),
        ];
        let cfg = build_cfg(&function(body));
        assert_eq!(cfg.blocks.len(), 3);
        assert_eq!(cfg.entry, 0);
        assert_eq!(cfg.blocks[0].successors.len(), 2);
        assert_eq!(cfg.blocks[0].successors[0].kind, EdgeKind::False);
        assert_eq!(cfg.blocks[0].successors[0].target, 2);
        assert_eq!(cfg.blocks[0].successors[1].kind, EdgeKind::True);
        assert_eq!(cfg.blocks[0].successors[1].target, 1);
        assert!(cfg.blocks[1].successors.is_empty());
        assert!(cfg.blocks[2].successors.is_empty());
        assert!(cfg.unresolved_targets.is_empty());
    }
}
