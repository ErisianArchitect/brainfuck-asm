use ::std::{
    ops::{
        Range,
    },
    path::{Path, PathBuf},
};


#[derive(Debug, Clone)]
pub struct ChunkRef<'a> {
    pub source: &'a str,
    pub code: &'a [u8],
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub source: Box<str>,
    pub code: Box<[u8]>,
    pub range: Range<usize>,
}

impl Chunk {
    pub fn from_ref(chunk: &ChunkRef<'_>) -> Self {
        Self {
            source: chunk.source.into(),
            code: chunk.code.into(),
            range: chunk.range.clone(),
        }
    }

    pub fn to_ref(&self) -> ChunkRef<'_> {
        ChunkRef {
            source: &self.source,
            code: &self.code,
            range: self.range.clone(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Footer {
    pub instruction_start: u32,
    pub instruction_end: u32,
    pub chunks_start: u32,
    pub chunks_end: u32,
    pub chunk_count: u32,
}

fn u32_from_bytes(bytes: &[u8]) -> u32 {
    debug_assert!(bytes.len() >= 4);
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[0..4]);
    u32::from_ne_bytes(buf)
}

impl Footer {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() != 32 {
            return Err(());
        }
        Ok(Self {
            instruction_start: u32_from_bytes(&bytes[ 0.. 4]),
            instruction_end:   u32_from_bytes(&bytes[ 4.. 8]),
            chunks_start:      u32_from_bytes(&bytes[ 8..12]),
            chunks_end:        u32_from_bytes(&bytes[12..16]),
            chunk_count:       u32_from_bytes(&bytes[16..20]),
        })
    }

    pub fn from_data_footer(data: &[u8]) -> Result<Self, ()> {
        if data.len() < 32 {
            return Err(());
        }
        let footer = &data[data.len() - 32..data.len()];
        Self::from_bytes(footer)
    }
}

// ;--------
// ; # FORMAT SPECIFICATION
// ; === VERSION[0.1.0]
// ; ### Footer
// ; 32-byte footer at the end of the file.
// ; The footer contains the following information:
// ; ```
// ; #[repr(C, align(32)]
// ; struct Footer {
// ;     instruction_start: u32,      (00..04)
// ;     instruction_end  : u32,      (04..08)
// ;     chunks_start     : u32,      (08..12)
// ;     chunk_count      : u32,      (12..16)
// ;     _reserved        : [u32; 4], (16..32)
// ; }
// ; ```
// ; The section where the instructions are stored starts
// ; at  `instruction_start`, and ends at `instruction_end`.
// ; The section where the chunks are stored starts
// ; at `chunks_start` and ends at `chunks_end`.
// ; The chunk_count is provided for your convenience.
// ; 12 bytes are reserved for alignment, padding, and
// ; future use.
// ;
// ; ### Chunks
// ; Chunks store the source code as text as well as
// ; offset information where the machine code is stored.
// ; ```
// ; struct Chunk {
// ;    source_len: u16,
// ;    source: str[source_len],
// ;    // Null byte for compatibility with C-strings.
// ;    _null_end: 0u8,
// ;    code_start: u32,
// ;    code_end: u32,
// ; }
// ; ```
// ;--------

pub struct ChunkReader<'a> {
    data: &'a [u8],
    read_offset: usize,
    chunks_end: usize,
    chunk_count: usize,
    chunk_index: usize,
}

impl<'a> ChunkReader<'a> {
    pub fn from_footer(data: &'a [u8], footer: &Footer) -> Result<Self, ()> {
        Ok(Self {
            data,
            read_offset: footer.chunks_start as usize,
            chunks_end: footer.chunks_end as usize,
            chunk_count: footer.chunk_count as usize,
            chunk_index: 0,
        })
    }
    
    pub fn new(data: &'a [u8]) -> Result<Self, ()> {
        let footer = Footer::from_data_footer(data)?;
        Self::from_footer(data, &footer)
    }

    fn remaining(&self) -> usize {
        self.chunks_end - self.read_offset
    }

    fn advance(&mut self, count: usize) -> Result<(), ()> {
        if self.remaining() < count {
            return Err(());
        }
        self.read_offset += count;
        Ok(())
    }

    fn borrow_exact(&mut self, length: usize) -> Result<&'a [u8], ()> {
        if self.remaining() < length {
            return Err(());
        }
        let buf = &self.data[self.read_offset..self.read_offset + length];
        self.advance(length).unwrap();
        Ok(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ()> {
        if self.remaining() < buf.len() {
            return Err(());
        }
        buf.copy_from_slice(&self.data[self.read_offset..self.read_offset + buf.len()]);
        self.advance(buf.len()).unwrap();
        Ok(())
    }

    fn read_bytes<const LEN: usize>(&mut self) -> Result<[u8; LEN], ()> {
        let mut buf = [0u8; LEN];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_byte(&mut self) -> Result<u8, ()> {
        Ok(self.read_bytes::<1>()?[0])
    }

    fn read_bool(&mut self) -> Result<bool, ()> {
        let byte = self.read_byte()?;
        Ok(byte != 0)
    }

    fn read_u16(&mut self) -> Result<u16, ()> {
        Ok(u16::from_ne_bytes(self.read_bytes()?))
    }

    fn read_u32(&mut self) -> Result<u32, ()> {
        Ok(u32::from_ne_bytes(self.read_bytes()?))
    }

    fn read_u32_range_as_usize(&mut self) -> Result<Range<usize>, ()> {
        let start = self.read_u32()?;
        let end = self.read_u32()?;
        Ok(start as usize..end as usize)
    }

    fn read_str(&mut self) -> Result<&'a str, ()> {
        let str_len = self.read_u16()? as usize;
        let str_bytes = self.borrow_exact(str_len)?;
        // Skip null byte.
        self.advance(1)?;
        match str::from_utf8(str_bytes) {
            Ok(s) => Ok(s),
            Err(_) => Err(()),
        }
    }

    pub fn read_chunk(&mut self) -> Option<ChunkRef<'a>> {
        if self.remaining() == 0 {
            return None;
        }
        let source = self.read_str().ok()?;
        let range = self.read_u32_range_as_usize().ok()?;
        let code = &self.data[range.clone()];
        self.chunk_index += 1;
        Some(ChunkRef {
            source,
            code,
            range,
        })
    }
} 

impl<'a> Iterator for ChunkReader<'a> {
    type Item = ChunkRef<'a>;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.read_chunk()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.chunk_count - self.chunk_index, Some(self.chunk_count - self.chunk_index))
    }
}

pub fn read_chunk_refs(data: &[u8]) -> Result<Vec<ChunkRef<'_>>, ()> {
    Ok(ChunkReader::new(data)?.collect::<Vec<_>>())
}

pub fn read_chunks(data: &[u8]) -> Result<Vec<Chunk>, ()> {
    Ok(
        ChunkReader::new(data)?
            .map(|chunk| {Chunk::from_ref(&chunk)})
            .collect::<Vec<_>>()
    )
}
