use ::std::{
    env,
    process::exit,
    path::{Path, PathBuf},
    fs::read,
    fs::metadata,
    io::{
        IsTerminal,
    },
    ops::{
        Range,
    },
};

use ::read_reveng::{
    format::read_chunk_refs,
};

mod error {
    use std::io::IsTerminal;

    #[derive(Clone, Copy)]
    pub struct Error<'a> {
        exit_code: u8,
        msg: &'a str,
    }

    impl<'a> Error<'a> {
        pub const fn new(exit_code: u8, msg: &'a str) -> Self {
            Self {
                exit_code,
                msg,
            }
        }

        pub fn exit(self, alt_message: Option<&str>) -> ! {
        const ERROR_PREFIX: &'static str = "\x1b[38;2;255;0;0mError\x1b[39m: ";
            if std::io::stderr().is_terminal() {
                let message = if let Some(msg) = alt_message {
                    msg
                } else {
                    self.msg
                };
                eprintln!("{ERROR_PREFIX}{message}");
            }
            ::std::process::exit(self.exit_code as i32)
        }
    }

    type ConstErr = Error<'static>;

    macro_rules! make_err {
        ($($name:ident($exit_code:expr, $message:expr);)*) => {
            ::paste::paste! {
                $(
                    #[inline(always)]
                    pub fn [< $name:lower >](alt_message: Option<&str>) -> ! {
                        const [< $name:upper >] : ConstErr = Error::new($exit_code, $message);
                        
                        [< $name:upper >].exit(alt_message)
                    }
                )*
            }
        };
    }

    make_err!{
        not_enough_args(1, "Not enough arguments.");
        too_many_args(2, "Too many arguments.");
        file_not_found(3, "File not found.");
        not_a_file(4, "Not a file.");
        not_implemented(69, "Not implemented.");
    }
    
}

fn str_to_path(s: &str) -> &Path {
    unsafe { ::core::mem::transmute(s) }
}

#[derive(Debug, Clone, Copy)]
pub struct Alt<T> {
    alts: [T; 2],
    selection: bool,
}

impl<T> Alt<T> {
    pub fn new(a: T, b: T) -> Self {
        Self {
            alts: [a, b],
            selection: false,
        }
    }

    pub fn alternate(&mut self) -> &T {
        let selection = &self.alts[self.selection as usize];
        self.selection ^= true;
        selection
    }

    pub fn alternate_mut(&mut self) -> &mut T {
        let selection = &mut self.alts[self.selection as usize];
        self.selection ^= true;
        selection
    }
}

const HEX_DIGITS: [u8; 16] = [b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'A', b'B', b'C', b'D', b'E', b'F'];

const fn byte_hex(byte: u8) -> [u8; 2] {
    let lower = byte & 0xF;
    let upper = byte >> 4;
    [HEX_DIGITS[upper as usize], HEX_DIGITS[lower as usize]]
}

// const BYTE_HEX: [[u8; 2]; 256] = {
//     let mut buf = [[0u8; 2]; 256];
//     let mut index = 0;
//     while index < 256 {
//         buf[index] = hex_byte(index as u8);
//         index += 1;
//     }
//     buf
// };

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn gray(gray: u8) -> Self {
        Self::new(gray, gray, gray)
    }

    pub fn fg_ansi(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    pub fn bg_ansi(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }

    pub fn wrap_fg<T: std::fmt::Display>(self, inner: &T) -> String {
        format!("\x1b[38;2;{};{};{}m{}\x1b[39m", self.r, self.g, self.b, inner)
    }

    pub fn wrap_bg<T: std::fmt::Display>(self, inner: &T) -> String {
        format!("\x1b[48;2;{};{};{}m{}\x1b[49m", self.r, self.g, self.b, inner)
    }
    
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg1: Rgb,
    pub bg2: Rgb,
    pub fg1: Rgb,
    pub fg2: Rgb,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteClass {
    Control = 0,
    Printable = 1,
    NonAscii = 2,
}

impl ByteClass {
    const CONTROL_FG: Rgb = Rgb::new(240, 110, 10);  // Orange
    const PRINTABLE_FG: Rgb = Rgb::new(10, 120, 40); // Green
    const NON_ASCII_FG: Rgb = Rgb::new(230, 230, 250); // White
    
    pub fn of(byte: u8) -> Self {
        match byte {
              0.. 32  => ByteClass::Control,
             32.. 127 => ByteClass::Printable,
            127       => ByteClass::Control,
            128..=255 => ByteClass::NonAscii,
        }
    }

    pub fn color(self) -> Rgb {
        match self {
            ByteClass::Control => Self::CONTROL_FG,
            ByteClass::Printable => Self::PRINTABLE_FG,
            ByteClass::NonAscii => Self::NON_ASCII_FG,
        }
    }
}

fn hex_ansi(bytes: &[u8], mut bgs: Alt<Rgb>, out: &mut String) {
    let mut add_spacer = false;
    for byte in bytes.iter().copied() {
        let hex = byte_hex(byte);
        let hex_str = unsafe { str::from_utf8_unchecked(&hex) };
        let bg = *bgs.alternate();
        let class = ByteClass::of(byte);
        let fg = class.color();
        use std::fmt::Write;
        if add_spacer {
            write!(out, " ").unwrap();
        } else {
            add_spacer = true;
        }
        write!(out, "\x1b[48;2;{};{};{}m\x1b[38;2;{};{};{}m{}\x1b[39m\x1b[49m", bg.r, bg.g, bg.b, fg.r, fg.g, fg.b, hex_str).unwrap();
    }
}

fn colorview() {
    // 4889C0
    const SOURCE: &'static str = "mov rax, rax";
    const DATA: &'static [u8] = &[0x48, 0x89, 0xC0];
    
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.len() {
        0 => {
            error::not_enough_args(None);
        }
        1 => {
            let input_path = args[0].as_str();
            let path = str_to_path(input_path);
            if !path.exists() {
                error::file_not_found(None);
            }
            if !path.is_file() {
                error::not_a_file(None);
            }
            let data = std::fs::read(path).expect("Failed to read file.");
            let chunks = read_chunk_refs(&data).expect("Failed to read the chunks");
            if chunks.is_empty() {
                println!("No chunks read.");
            } else {
                let mut longest_line = 0usize;
                // 0000..0000
                // ####@@@@####@@@@####@@@@####
                // mov rax, rax    0..3        4889C0
                // first pass through to perform layout.
                for chunk in chunks.iter() {
                    longest_line = longest_line.max(chunk.source.len());
                }
                let source_width = (longest_line + 4).next_multiple_of(4);
                fn emit_spaces(count: usize) {
                    const SPACES: &'static str = "                                ";
                    let mut remaining = count;
                    while remaining > 0 {
                        if remaining >= SPACES.len() {
                            print!("{}", SPACES);
                            remaining -= SPACES.len();
                        } else {
                            print!("{}", &SPACES[..remaining]);
                            break;
                        }
                    }
                }
                let mut alt = Alt::new(Rgb::gray(0x2c), Rgb::gray(0x0b));
                for chunk in chunks.iter() {
                    let bg = *alt.alternate();
                    print!("{}", bg.bg_ansi());
                    print!("{}", chunk.source);
                    let space_count = source_width - chunk.source.len();
                    emit_spaces(space_count);
                    let range_text = format!("{}..{}", chunk.range.start, chunk.range.end);
                    print!("{range_text}");
                    let space_count = 12 - range_text.len();
                    emit_spaces(space_count);
                    let mut output = String::with_capacity(256);
                    hex_ansi(chunk.code, Alt::new(Rgb::gray(0x37), Rgb::gray(0x21)), &mut output);
                    println!("{}", output);
                }
                println!("--------");
            }
        }
        2 => {
            error::not_implemented(None);
            let input = args[0].as_str();
            let output = args[1].as_str();
            let input_path = str_to_path(input);
            let output_path = str_to_path(output);
            if !input_path.exists() {
                let error_message = format!("File not found (\"{input}\")");
                error::file_not_found(Some(&error_message));
            }
            if !input_path.is_file() {
                let error_message = format!("Not a file (\"{input}\")");
                error::not_a_file(Some(&error_message));
            }
            if !output_path.exists() {
                let error_message = format!("File not found (\"{output}\")");
                error::file_not_found(Some(&error_message));
            }
            if !output_path.is_file() {
                let error_message = format!("Not a file (\"{output}\")");
                error::not_a_file(Some(&error_message));
            }
        }
        _ => {
            error::too_many_args(None);
        }
    }
}
