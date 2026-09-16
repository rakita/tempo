use crate::{evm::TempoContext, gas_credits};
use alloy_evm::Database;
use revm::{
    bytecode::opcode::{
        ADDMOD, KECCAK256, MOD, MULMOD, NOT, SAR, SELFBALANCE, SHL, SHR, SMOD, SSTORE,
    },
    handler::instructions::EthInstructions,
    interpreter::{
        Instruction, InstructionContext, InstructionResult,
        instructions::{gas_table_spec, instruction_table},
        interpreter::EthInterpreter,
        push,
    },
};
use tempo_chainspec::hardfork::TempoHardfork;

/// Instruction ID for opcode returning milliseconds timestamp.
const MILLIS_TIMESTAMP: u8 = 0x4F;

/// Gas cost for [`MILLIS_TIMESTAMP`] instruction. Same as other opcodes accessing block information.
const MILLIS_TIMESTAMP_GAS_COST: u16 = 2;

/// Alias for Tempo-specific [`InstructionContext`].
type TempoInstructionContext<'a, DB> = InstructionContext<'a, TempoContext<DB>, EthInterpreter>;

/// Opcode returning current timestamp in milliseconds.
fn millis_timestamp<DB: Database>(
    context: TempoInstructionContext<'_, DB>,
) -> Result<(), InstructionResult> {
    push!(context.interpreter, context.host.block.timestamp_millis());
    Ok(())
}

/// Returns configured instructions table for Tempo.
pub(crate) fn tempo_instructions<DB: Database>(
    spec: TempoHardfork,
) -> EthInstructions<EthInterpreter, TempoContext<DB>> {
    let evm_spec = spec.into();

    // +T7: Enable TIP-1060 sstore hook
    let mut instructions = if spec.is_t7() {
        EthInstructions::new(
            {
                let mut table = instruction_table::<EthInterpreter, TempoContext<DB>>();
                table[SSTORE as usize] = Instruction::new(gas_credits::sstore);
                table
            },
            gas_table_spec(evm_spec),
            evm_spec,
        )
    } else {
        EthInstructions::new_mainnet_with_spec(spec.into())
    };

    if !spec.is_t1c() {
        instructions.insert_instruction(
            MILLIS_TIMESTAMP,
            Instruction::new(millis_timestamp),
            MILLIS_TIMESTAMP_GAS_COST,
        );
    }

    if spec.is_t13() {
        // TIP-1102: static opcode repricing. KECCAK256's dynamic per-word
        // component is configured in `tempo_gas_params`.
        for opcode in [MOD, SMOD] {
            instructions.insert_gas(opcode, 25);
        }
        for opcode in [ADDMOD, MULMOD] {
            instructions.insert_gas(opcode, 30);
        }
        for opcode in [SHL, SHR, SAR] {
            instructions.insert_gas(opcode, 5);
        }
        instructions.insert_gas(NOT, 4);
        instructions.insert_gas(KECCAK256, 150);
        instructions.insert_gas(SELFBALANCE, 20);
    }
    instructions
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::database::EmptyDB;

    #[test]
    fn tip_1102_opcode_prices_activate_at_t13() {
        let t12 = tempo_instructions::<EmptyDB>(TempoHardfork::T12);
        let t13 = tempo_instructions::<EmptyDB>(TempoHardfork::T13);

        for (opcode, old, new) in [
            (MOD, 5, 25),
            (SMOD, 5, 25),
            (ADDMOD, 8, 30),
            (MULMOD, 8, 30),
            (NOT, 3, 4),
            (SHL, 3, 5),
            (SHR, 3, 5),
            (SAR, 3, 5),
            (KECCAK256, 30, 150),
            (SELFBALANCE, 5, 20),
        ] {
            assert_eq!(t12.gas_table()[opcode as usize], old);
            assert_eq!(t13.gas_table()[opcode as usize], new);
        }
    }
}
