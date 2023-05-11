use alloc::vec::Vec;
use ark_ff::Field;
use ark_ff::PrimeField;
use layouts::layout6::Flag;
use num_bigint::BigUint;
use ruint::aliases::U256;
use ruint::uint;
use serde::Deserialize;
use serde::Serialize;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

// https://eprint.iacr.org/2021/1063.pdf figure 3
/// Word offset of `off_DST`
pub const OFF_DST_BIT_OFFSET: usize = 0;
/// Word offset of `off_OP0`
pub const OFF_OP0_BIT_OFFSET: usize = 16;
/// Word offset of `off_OP1`
pub const OFF_OP1_BIT_OFFSET: usize = 32;
/// Word offset of instruction flags
pub const FLAGS_BIT_OFFSET: usize = 48;

/// Number of Cairo instruction flags
pub const _NUM_FLAGS: usize = 16;

// Mask for word offsets (16 bits each)
pub const OFF_MASK: usize = 0xFFFF;

pub const _OFFSET: usize = 2usize.pow(16);
pub const HALF_OFFSET: usize = 2usize.pow(15);

/// Holds register values
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegisterState {
    pub ap: usize,
    pub fp: usize,
    pub pc: usize,
}

pub struct RegisterStates(Vec<RegisterState>);

impl RegisterStates {
    /// Parses trace data in the format outputted by a `cairo-run`.
    pub fn from_reader(r: impl Read) -> Self {
        // TODO: errors
        let mut reader = BufReader::new(r);
        let mut register_states = Vec::new();
        while reader.has_data_left().unwrap() {
            let entry: RegisterState = bincode::deserialize_from(&mut reader).unwrap();
            register_states.push(entry);
        }
        RegisterStates(register_states)
    }
}

impl Deref for RegisterStates {
    type Target = Vec<RegisterState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct Memory<F>(Vec<Option<Word<F>>>);

impl<F: Field> Memory<F> {
    /// Parses the partial memory data outputted by a `cairo-run`.
    pub fn from_reader(r: impl Read) -> Self
    where
        F: PrimeField,
    {
        // TODO: errors
        // TODO: each builtin has its own memory segment.
        // check it also contains other builtins
        // this file contains the contiguous memory segments:
        // - program
        // - execution
        // - builtin 0
        // - builtin 1
        // - ...
        let mut reader = BufReader::new(r);
        let mut partial_memory = Vec::new();
        let mut max_address = 0;
        while reader.has_data_left().unwrap() {
            // TODO: ensure always deserializes u64 and both are always little-endian
            let address = bincode::deserialize_from(&mut reader).unwrap();
            // TODO: U256 bincode has memory overallocation bug
            let word_bytes: [u8; 32] = bincode::deserialize_from(&mut reader).unwrap();
            let word = U256::from_le_bytes(word_bytes);
            partial_memory.push((address, Word::new(word)));
            max_address = std::cmp::max(max_address, address);
        }

        // TODO: DOC: None used for nondeterministic values?
        let mut memory = vec![None; max_address + 1];
        for (address, word) in partial_memory {
            // TODO: once arkworks v4 release remove num_bigint
            memory[address] = Some(word);
        }

        Memory(memory)
    }
}

impl<F: Field> Deref for Memory<F> {
    type Target = Vec<Option<Word<F>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct CompiledProgram<F> {
    data: Vec<String>,
    prime: String,
    _marker: PhantomData<F>,
}

impl<F: PrimeField> CompiledProgram<F> {
    // TODO: could use https://github.com/Keats/validator instead of calling this everywhere
    // but seems a bit heave to add as a dependency just to do this
    pub fn validate(&self) {
        // Make sure the field modulus matches the expected
        let modulus: BigUint = F::MODULUS.into();
        assert_eq!(format!("{:#x}", modulus), self.prime.to_lowercase());
    }

    pub fn get_public_memory(&self) -> Vec<(usize, F)> {
        self.data
            .iter()
            .enumerate()
            .map(|(i, value_str)| {
                (
                    i + 1, // address 0, 0 is reserved for dummy accesses
                    Word::new(U256::from_str(value_str).expect("invalid data item")).into_felt(),
                )
            })
            .collect()
    }

    pub fn get_padding_address_and_value(&self) -> (usize, F) {
        // TODO: make more concrete. By convention seems to be next after public memory
        let address = self.data.len() + 1;
        (address, (address as u64).into())
    }
}

impl<'a, F> Deserialize<'a> for CompiledProgram<F> {
    // Have to implement custom deserialize because serde panics with PhantomData
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        #[derive(Deserialize)]
        struct Program {
            data: Vec<String>,
            prime: String,
        }

        let program = Program::deserialize(deserializer)?;
        Ok(Self {
            data: program.data,
            prime: program.prime,
            _marker: PhantomData,
        })
    }
}

/// Represents a Cairo word
/// Value is a field element in the range `[0, Fp::MODULUS)`
/// Stored as a U256 to make binary decompositions more efficient
#[derive(Clone, Copy, Debug)]
pub struct Word<F>(pub U256, PhantomData<F>);

impl<F> Word<F> {
    /// Calculates $\tilde{f_i}$ - https://eprint.iacr.org/2021/1063.pdf
    pub fn get_flag_prefix(&self, flag: Flag) -> u64 {
        if flag == Flag::Zero {
            return 0;
        }

        let flag = flag as usize;
        let prefix = self.0 >> (FLAGS_BIT_OFFSET + flag);
        let mask = (uint!(1_U256) << (15 - flag)) - uint!(1_U256);
        (prefix & mask).try_into().unwrap()
    }

    pub fn get_op0_addr(&self, ap: usize, fp: usize) -> usize {
        // TODO: put the if statement first good for rust quiz
        self.get_off_op0() + if self.get_flag(Flag::Op0Reg) { fp } else { ap } - HALF_OFFSET
    }

    pub fn get_dst_addr(&self, ap: usize, fp: usize) -> usize {
        self.get_off_dst() + if self.get_flag(Flag::DstReg) { fp } else { ap } - HALF_OFFSET
    }

    pub fn get_flag(&self, flag: Flag) -> bool {
        self.0.bit(FLAGS_BIT_OFFSET + flag as usize)
    }

    pub fn get_off_dst(&self) -> usize {
        let prefix = self.0 >> OFF_DST_BIT_OFFSET;
        let mask = U256::from(OFF_MASK);
        (prefix & mask).try_into().unwrap()
    }

    pub fn get_off_op0(&self) -> usize {
        let prefix = self.0 >> OFF_OP0_BIT_OFFSET;
        let mask = U256::from(OFF_MASK);
        (prefix & mask).try_into().unwrap()
    }

    pub fn get_off_op1(&self) -> usize {
        let prefix = self.0 >> OFF_OP1_BIT_OFFSET;
        let mask = U256::from(OFF_MASK);
        (prefix & mask).try_into().unwrap()
    }

    pub fn get_flag_group(&self, flag_group: FlagGroup) -> u8 {
        match flag_group {
            FlagGroup::DstReg => self.get_flag(Flag::DstReg) as u8,
            FlagGroup::Op0Reg => self.get_flag(Flag::Op0Reg) as u8,
            FlagGroup::Op1Src => {
                self.get_flag(Flag::Op1Imm) as u8
                    + self.get_flag(Flag::Op1Fp) as u8 * 2
                    + self.get_flag(Flag::Op1Ap) as u8 * 4
            }
            FlagGroup::ResLogic => {
                self.get_flag(Flag::ResAdd) as u8 + self.get_flag(Flag::ResMul) as u8 * 2
            }
            FlagGroup::PcUpdate => {
                self.get_flag(Flag::PcJumpAbs) as u8
                    + self.get_flag(Flag::PcJumpRel) as u8 * 2
                    + self.get_flag(Flag::PcJnz) as u8 * 4
            }
            FlagGroup::ApUpdate => {
                self.get_flag(Flag::ApAdd) as u8 + self.get_flag(Flag::ApAdd1) as u8 * 2
            }
            FlagGroup::Opcode => {
                self.get_flag(Flag::OpcodeCall) as u8
                    + self.get_flag(Flag::OpcodeRet) as u8 * 2
                    + self.get_flag(Flag::OpcodeAssertEq) as u8 * 4
            }
        }
    }
}

impl<F: PrimeField> Word<F> {
    pub fn new(word: U256) -> Self {
        let modulus: BigUint = F::MODULUS.into();
        debug_assert!(BigUint::from(word) < modulus);
        Word(word, PhantomData)
    }

    pub fn get_op0(&self, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        mem[self.get_op0_addr(ap, fp)].unwrap().into_felt()
    }

    pub fn get_dst(&self, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        mem[self.get_dst_addr(ap, fp)].unwrap().into_felt()
    }

    pub fn get_op1_addr(&self, pc: usize, ap: usize, fp: usize, mem: &Memory<F>) -> usize {
        self.get_off_op1()
            + match self.get_flag_group(FlagGroup::Op1Src) {
                0 => usize::try_from(mem[self.get_op0_addr(ap, fp)].unwrap().0).unwrap(),
                1 => pc,
                2 => fp,
                4 => ap,
                _ => unreachable!(),
            }
            - HALF_OFFSET
    }

    pub fn get_op1(&self, pc: usize, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        mem[self.get_op1_addr(pc, ap, fp, mem)].unwrap().into_felt()
    }

    pub fn get_res(&self, pc: usize, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        let pc_update = self.get_flag_group(FlagGroup::PcUpdate);
        let res_logic = self.get_flag_group(FlagGroup::ResLogic);
        match pc_update {
            4 => {
                let opcode = self.get_flag_group(FlagGroup::Opcode);
                let ap_update = self.get_flag_group(FlagGroup::ApUpdate);
                if res_logic == 0 && opcode == 0 && ap_update != 1 {
                    // From the Cairo whitepaper "We use the term Unused to
                    // describe a variable that will not be used later in the
                    // flow. As such, we don’t need to assign it a concrete
                    // value.". Note `res` is repurposed when calculating next_pc and
                    // stores the value of `dst^(-1)` (see air.rs for more details).
                    self.get_dst(ap, fp, mem).inverse().unwrap_or_else(F::zero)
                } else {
                    unreachable!()
                }
            }
            0 | 1 | 2 => {
                let op0: F = mem[self.get_op0_addr(ap, fp)].unwrap().into_felt();
                let op1: F = mem[self.get_op1_addr(pc, ap, fp, mem)].unwrap().into_felt();
                match res_logic {
                    0 => op1,
                    1 => op0 + op1,
                    2 => op0 * op1,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }

    pub fn get_tmp0(&self, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        if self.get_flag(Flag::PcJnz) {
            self.get_dst(ap, fp, mem)
        } else {
            // TODO: change
            F::zero()
        }
    }

    pub fn get_tmp1(&self, pc: usize, ap: usize, fp: usize, mem: &Memory<F>) -> F {
        self.get_tmp0(ap, fp, mem) * self.get_res(pc, ap, fp, mem)
    }

    pub fn into_felt(self) -> F {
        BigUint::from(self.0).into()
    }
}

/// Cairo flag group
/// https://eprint.iacr.org/2021/1063.pdf section 9.4
#[derive(Clone, Copy)]
pub enum FlagGroup {
    DstReg,
    Op0Reg,
    Op1Src,
    ResLogic,
    PcUpdate,
    ApUpdate,
    Opcode,
}
