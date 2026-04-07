use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::account_info::{AccountInfo, next_account_info};
use solana_program::pubkey::Pubkey;
use solana_program::entrypoint::ProgramResult;
use solana_program::entrypoint;

entrypoint!(dev_contract);

#[derive(BorshSerialize, BorshDeserialize)]
enum Operation {
    Add(u32),
    Sub(u32),
}

#[derive(BorshSerialize, BorshDeserialize)]
struct Counter {
    count: u32,
}

pub fn dev_contract(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
        : &[u8],
) -> ProgramResult {

    let acc = next_account_info(&mut accounts.iter())?;

    let ope_type = Operation::try_from_slice(instruction_data)?;

    // MUST be mutable
    let mut count = Counter::try_from_slice(&acc.data.borrow())?;

    match ope_type {
        Operation::Add(val) => {
            count.count += val;
        }
        Operation::Sub(val) => {
            count.count -= val;
        }
    }

    // MUST use borrow_mut
    count.serialize(&mut *acc.data.borrow_mut())?;

    Ok(())
}
