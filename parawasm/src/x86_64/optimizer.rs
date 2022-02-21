use alloc::vec::Vec;
use core::mem::replace;
use iced_x86::code_asm::{CodeAssembler, CodeLabel};
use iced_x86::{Code, Instruction, OpKind};

pub(crate) fn optimize(
    instructions: Vec<Instruction>,
    labels: &mut Vec<(usize, CodeLabel)>,
) -> Result<Vec<Instruction>, super::Error> {
    let mut current_instructions = instructions;
    loop {
        // Optimize
        let opt_instructions = optimize_loop(current_instructions.clone(), labels)?;

        // If no further optimization occurred, return the result
        if &opt_instructions == &current_instructions {
            return Ok(current_instructions);
        }

        // Prepare to optimize again
        current_instructions = opt_instructions;
    }
}

pub(crate) fn optimize_loop(
    instructions: Vec<Instruction>,
    labels: &mut Vec<(usize, CodeLabel)>,
) -> Result<Vec<Instruction>, super::Error> {
    let mut labels_ = labels
        .iter()
        .map(|(index, label)| (*index, *index, *label))
        .collect::<Vec<_>>();
    let mut new_instructions = CodeAssembler::new(64)?;
    let mut head = instructions.as_slice();
    let mut head_idx = 0;
    while head.len() > 0 {
        if head.len() >= 2 {
            // PUSH reg + POP reg
            if head[0].code() == Code::Push_r64 && head[1].code() == Code::Pop_r64 {
                new_instructions.add_instruction(Instruction::with2(
                    Code::Mov_rm64_r64,
                    head[1].op0_register(),
                    head[0].op0_register(),
                )?)?;
                head = &head[2..];
                head_idx += 2;
                update_labels(&mut labels_, head_idx, -1);
                continue;
            }

            // MOV reg1, reg2 + MOV reg2, reg1
            if head[0].code() == Code::Mov_rm64_r64
                && head[1].code() == Code::Mov_rm64_r64
                && head[0].op0_kind() == OpKind::Register
                && head[0].op1_kind() == OpKind::Register
                && head[1].op0_kind() == OpKind::Register
                && head[1].op1_kind() == OpKind::Register
                && head[0].op0_register() == head[1].op1_register()
                && head[1].op0_register() == head[0].op1_register()
            {
                head = &head[2..];
                head_idx += 2;
                update_labels(&mut labels_, head_idx, -2);

                continue;
            }
        }
        if head.len() >= 1 {
            // MOV reg, reg
            if head[0].code() == Code::Mov_rm64_r64
                && head[0].op0_kind() == OpKind::Register
                && head[0].op1_kind() == OpKind::Register
                && head[0].op0_register() == head[0].op1_register()
            {
                head = &head[1..];
                head_idx += 1;
                update_labels(&mut labels_, head_idx, -1);
                continue;
            }
        }

        new_instructions.add_instruction(head[0])?;

        head = &head[1..];
        head_idx += 1;
    }
    let _ = replace(
        labels,
        labels_
            .into_iter()
            .map(|(_, index, label)| (index, label))
            .collect(),
    );
    Ok(new_instructions.take_instructions())
}

fn update_labels(labels: &mut Vec<(usize, usize, CodeLabel)>, head_idx: usize, count: isize) {
    for (original_index, index, _label) in labels.iter_mut() {
        if *original_index >= head_idx {
            *index = (*index as isize + count) as usize;
        }
    }
}
