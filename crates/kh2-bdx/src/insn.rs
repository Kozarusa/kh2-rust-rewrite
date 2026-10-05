//! Instruction decoding. Every instruction is one 16-bit word (low 4 bits: major opcode, bits 4-5:
//! operand type, bits 6-15: sub-operation or addressing mode) followed by zero to three operand words.

use crate::program::Program;

/// How an operand word is turned into an address (bits 6-15 of the instruction word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// Frame (local variable) memory: `frame + offset`.
    Local,
    /// Global memory shared by the thread group: `globals + offset`.
    Global,
    /// `*(frame[0]) + offset`: slot 0 of the frame holds a pointer.
    LocalPtr,
    /// Inside the program image: `code + 2 * offset` (constants, strings, argument blocks).
    Code,
    /// Any other mode value: the operand is a plain address. The game computes `0 + offset`.
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operand {
    pub base: Base,
    pub offset: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Insn {
    /// Push a 32-bit constant stored in the next two words (integer).
    PushInt(u32),
    /// Same encoding, float typed.
    PushFloat(f32),
    /// Push the address of an operand.
    Lea(Operand),
    /// Push the 32-bit value at an operand address.
    Load(Operand),
    /// Pop a value into the 32-bit slot at an operand address.
    Store(Operand),
    /// Pop a source address; copy `size` bytes from it to the operand address.
    CopyTo { dst: Operand, size: u16 },
    /// Pop an address, push the 32-bit value at `address + offset` (field load through a pointer).
    LoadField(u16),
    /// Pop a value, pop an address; store the value there (32 bit).
    StoreIndirect,
    /// Pop a source address, pop a destination address; copy `size` bytes.
    CopyIndirect { size: u16 },
    Unary { float: bool, op: UnaryOp },
    Binary { float: bool, op: BinaryOp },
    /// Relative jump (target already resolved to a code word index).
    Jump(usize),
    /// Pop; jump when zero.
    JumpIfZero(usize),
    /// Pop; jump when non-zero.
    JumpIfNonZero(usize),
    /// Call a routine. The caller's frame is `frame_words` 32-bit words, the last two of which hold the
    /// return linkage; the callee's locals start right after them.
    Call { frame_words: u16, target: usize },
    /// Suspend the thread until the next update (run result 1).
    Yield,
    /// End the thread (run result 2; the game ends a thread on every result other than "yield").
    Exit,
    /// Return; returning from the outermost frame ends the thread (run result 3).
    Ret,
    /// Discard the top of the operand stack.
    Pop,
    /// Duplicate the top of the operand stack.
    Dup,
    /// Pop radians (wrapped to -pi..pi), push sin.
    Sin,
    /// Pop radians (wrapped to -pi..pi), push cos.
    Cos,
    /// Pop degrees, push radians wrapped to -pi..pi.
    DegToRad,
    /// Pop radians, push degrees.
    RadToDeg,
    /// Call a host function: `group`, `index` select it; it pops its declared arguments.
    Host { group: u16, index: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// int: to float. float: to int (truncate).
    Convert,
    Neg,
    /// Bitwise not (int only).
    Not,
    /// `x == 0` as 0/1. int only (sub-operation 4 and 8 behave alike).
    IsZero,
    Abs,
    /// `x < 0`
    Lt0,
    /// `x <= 0`
    Le0,
    /// `x == 0` (float only)
    Eq0,
    /// `x != 0`
    Ne0,
    /// `x >= 0`
    Ge0,
    /// `x > 0`
    Gt0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    /// int: remainder. float: fmod.
    Rem,
    And,
    Or,
    Xor,
    Shl,
    /// Arithmetic shift right.
    Shr,
    /// Logical and of the two truth values.
    LogAnd,
    /// Logical or of the two truth values.
    LogOr,
}

fn unary(float: bool, sub: u16) -> Option<UnaryOp> {
    use UnaryOp::*;
    Some(match (float, sub) {
        (false, 0) | (true, 1) => Convert,
        (_, 2) => Neg,
        (false, 3) => Not,
        (false, 4) | (false, 8) => IsZero,
        (_, 5) => Abs,
        // Int sub-operation 6 and 10 test the sign bit of the value or its complement.
        (false, 6) => Lt0,
        (false, 10) => Ge0,
        (false, 7) => Le0,
        (false, 9) => Ne0,
        (false, 11) => Gt0,
        (true, 6) => Lt0,
        (true, 7) => Le0,
        (true, 8) => Eq0,
        (true, 9) => Ne0,
        (true, 10) => Ge0,
        (true, 11) => Gt0,
        _ => return None,
    })
}

fn binary(float: bool, sub: u16) -> Option<BinaryOp> {
    use BinaryOp::*;
    Some(match (float, sub) {
        (_, 0) => Add,
        (_, 1) => Sub,
        (_, 2) => Mul,
        (_, 3) => Div,
        (_, 4) => Rem,
        (false, 5) => And,
        (false, 6) => Or,
        (false, 7) => Xor,
        (false, 8) => Shl,
        (false, 9) => Shr,
        (false, 10) => LogAnd,
        (false, 11) => LogOr,
        _ => return None,
    })
}

fn base_of(mode: u16) -> Base {
    match mode {
        0 => Base::Local,
        1 => Base::Global,
        2 => Base::LocalPtr,
        3 => Base::Code,
        _ => Base::Absolute,
    }
}

/// Decodes the instruction at word index `pc`. Returns it with its length in words, or `None` when the
/// word (or an operand) is not a valid instruction.
pub fn decode(p: &Program, pc: usize) -> Option<(Insn, usize)> {
    let w = p.word(pc)?;
    let sub = w >> 6;
    let ty = (w >> 4) & 3;
    let arg = |k: usize| p.word(pc + k);
    let operand = |k: usize| Some(Operand { base: base_of(sub), offset: arg(k)? });
    let rel = |k: usize| -> Option<usize> { usize::try_from(pc as i64 + k as i64 + 1 + arg(k)? as i16 as i64).ok() };
    Some(match w & 0xf {
        0 => match ty {
            0 => (Insn::PushInt(p.u32_at_word(pc + 1)?), 3),
            1 => (Insn::PushFloat(f32::from_bits(p.u32_at_word(pc + 1)?)), 3),
            2 => (Insn::Lea(operand(1)?), 2),
            _ => (Insn::Load(operand(1)?), 2),
        },
        1 => (Insn::Store(operand(1)?), 2),
        2 => (Insn::CopyTo { size: arg(1)?, dst: operand(2)? }, 3),
        3 => (Insn::LoadField(arg(1)?), 2),
        4 => (if sub != 0 { Insn::CopyIndirect { size: sub } } else { Insn::StoreIndirect }, 1),
        5 => {
            if ty > 1 {
                return None;
            }
            (Insn::Unary { float: ty == 1, op: unary(ty == 1, sub)? }, 1)
        }
        6 => {
            if ty > 1 {
                return None;
            }
            (Insn::Binary { float: ty == 1, op: binary(ty == 1, sub)? }, 1)
        }
        7 => {
            let t = rel(1)?;
            match sub {
                0 => (Insn::Jump(t), 2),
                1 => (Insn::JumpIfZero(t), 2),
                2 => (Insn::JumpIfNonZero(t), 2),
                _ => return None,
            }
        }
        8 => (Insn::Call { frame_words: sub, target: rel(1)? }, 2),
        9 => (
            match sub {
                0 => Insn::Yield,
                1 => Insn::Exit,
                2 => Insn::Ret,
                3 => Insn::Pop,
                5 => Insn::Dup,
                6 => Insn::Sin,
                7 => Insn::Cos,
                8 => Insn::DegToRad,
                9 => Insn::RadToDeg,
                _ => return None,
            },
            1,
        ),
        10 => (Insn::Host { group: sub, index: arg(1)? }, 2),
        _ => return None,
    })
}
