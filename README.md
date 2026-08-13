# ELF64 (Linux) Brainfuck JIT Compiler Written in fasm g.

fasm g: [https://github.com/tgrysztar/fasmg]

This project is a work in progress. It's something I'm working on alongside several other projects, so don't
expect this to be completed too quickly. I've written some code to help me "reverse engineer" x86_64 assembly
so that I can emit it at runtime to JIT compile brainfuck. I'll try to make progress updates here in the
readme until the project is finished, at which point I'll probably change the entire readme to be a
description of the project.

- [x] Write code to reverse engineer x86_64 Assembly using fasmg.
- [ ] Plan how the JIT compiler will emit instructions.
- [ ] Decide what instruction sequences/patterns to collapse into optimized code.
- [ ] Allocate registers.
- [ ] Loading of brainfuck source from a file.
- [ ] Translation of the brainfuck source into bytecode.
- [ ] Translation of the bytecode into IR
- [ ] JIT compile assembly from IR using multiple passes.
