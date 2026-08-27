// SPDX-License-Identifier: LGPL-3.0-or-later

fn main() {
    sp1_build::build_program_with_args("../program", Default::default());
    sp1_build::build_program_with_args("../program-elephant", Default::default());
    sp1_build::build_program_with_args("../program-leaf", Default::default());
    sp1_build::build_program_with_args("../program-root", Default::default());
}
