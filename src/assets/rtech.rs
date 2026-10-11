//! RTech pak compression (header flag `0x100`): the format the original Season 3 paks still in
//! the install use. A whole pak is decoded in memory here, following the game's stream decoder
//! with its ring buffers opened out to the full file. The decoder's lookup table is read from the
//! install's own `r5apex.exe` (`exe::RTECH_LUT_RVA`).

use super::zstd::Error;

type Result<T> = std::result::Result<T, Error>;

const PAD: usize = 64;

fn rd64(d: &[u8], at: u64) -> u64 {
    let at = at as usize;
    let mut b = [0u8; 8];
    let n = d.len().saturating_sub(at).min(8);
    if n > 0 {
        b[..n].copy_from_slice(&d[at..at + n]);
    }
    u64::from_le_bytes(b)
}

fn lut32(lut: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(lut[at..at + 4].try_into().unwrap())
}

/// Copies `n` bytes as the game's 8-byte loads and stores do, in order, so an overlapping copy
/// repeats what it has just written.
fn copy8(out: &mut [u8], from: usize, to: usize, n: usize) {
    let mut k = 0;
    while k < n {
        let chunk: [u8; 8] = out[from + k..from + k + 8].try_into().unwrap();
        out[to + k..to + k + 8].copy_from_slice(&chunk);
        k += 8;
    }
}

/// Decodes a whole RTech-compressed pak (`src` is the file, header included). The 0x80-byte
/// header is copied through; the result is `decompressed` bytes long.
pub fn decompress(src: &[u8], lut: &[u8], decompressed: usize) -> Result<Vec<u8>> {
    const HEADER: u64 = 0x80;
    if lut.len() < 0x4e2 {
        return Err(Error("rtech lookup table too short"));
    }
    let input = src;
    let in_len = src.len() as u64;
    let mask = u64::MAX;
    let frame = rd64(input, HEADER);
    let bits = (frame & 0x3f) as u32;
    let frame = frame >> 6;
    let decomp_size = (1u64 << bits) | (frame & ((1u64 << bits) - 1));
    if decomp_size as usize != decompressed {
        return Err(Error("rtech frame size does not match the header"));
    }
    let byte_pos = HEADER + 8;
    let low = rd64(input, byte_pos) << (64 - (bits + 6));
    let mut in_pos = byte_pos + ((bits + 6) >> 3) as u64;
    let bit_final = ((bits + 6) & 7) + 13;
    let curr = (mask >> ((bits + 6) & 7)) & ((frame >> bits) | low);
    let currbits = (((curr as u8).wrapping_sub(1)) & 0x3f) as u32 + 1;
    let input_inv = mask >> (64 - currbits);
    let output_inv = mask >> (64 - ((((curr >> 6).wrapping_sub(1)) & 0x3f) + 1));
    let final_full = (curr >> 13) | (rd64(input, in_pos) << (64 - bit_final));
    let final_off = bit_final & 7;
    in_pos += (bit_final >> 3) as u64;
    let final_byte = (mask >> final_off) & final_full;
    let (header_off, mut size_needed);
    if input_inv == mask {
        header_off = 0u64;
        size_needed = in_len;
    } else {
        let at = in_pos;
        header_off = ((currbits >> 3) + 1) as u64;
        in_pos += header_off;
        size_needed = rd64(input, at) & ((1u64 << (8 * header_off)) - 1);
    }
    let mut cur_byte = final_byte;
    let mut cur_bit = final_off;
    let mut q70 = input_inv.wrapping_sub(6);
    let mut d6c: u32 = 0;
    let mut comp_stream = size_needed;
    let mut decomp_stream = decomp_size;
    if decomp_size - 1 > output_inv {
        comp_stream = size_needed - header_off;
        decomp_stream = output_inv + 1;
    }

    let mut out = vec![0u8; decompressed + PAD];
    out[..HEADER as usize].copy_from_slice(&src[..HEADER as usize]);
    let mut padded = Vec::with_capacity(input.len() + PAD);
    padded.extend_from_slice(input);
    padded.resize(input.len() + PAD, 0);
    let input = &padded[..];
    let mut out_pos = HEADER;
    let mut limit = if comp_stream < q70 { comp_stream } else { q70 };
    let mut v19: u64;
    let mut first = cur_bit == 0;
    loop {
        if !first {
            let v13 = (rd64(input, in_pos) << (64 - cur_bit)) | cur_byte;
            let i = cur_bit;
            cur_bit &= 7;
            in_pos += (i >> 3) as u64;
            cur_byte = (mask >> cur_bit) & v13;
        }
        first = false;
        let v15 = (d6c as usize) << 8;
        let v17 = lut[(cur_byte as u8) as usize + 512 + v15] as u32;
        let v18 = (cur_byte as u8) as usize + v15;
        cur_bit += v17;
        v19 = cur_byte >> v17;
        let v20 = lut[v18] as i8 as i32;
        if lut[v18] & 0x80 != 0 {
            let mut n = (-v20) as u32;
            let (from, to) = (in_pos as usize, out_pos as usize);
            d6c = 1;
            if n == lut[(v15 >> 8) + 1248] as u32 {
                if (!in_pos & input_inv) < 0xf || (output_inv & !out_pos) < 0xf || decomp_size - out_pos < 0x10 {
                    n = 1;
                }
                let v60 = v19 >> 3;
                let v61 = (v19 & 7) as usize;
                let (mut v62, v63, v64);
                if v61 != 0 {
                    v62 = v60;
                    v63 = lut[v61 + 1232] as u32;
                    v64 = lut[v61 + 1240] as u32;
                } else {
                    v62 = v60 >> 4;
                    let v65 = (v60 & 0xf) as usize;
                    cur_bit += 4;
                    v63 = lut32(lut, 4 * v65 + 1152);
                    v64 = lut[v65 + 1216] as u32;
                }
                cur_bit += v64 + 3;
                let low_bits = (v62 & ((1u64 << v64) - 1)) as u32;
                v62 >>= v64;
                v19 = v62;
                let v66 = v63.wrapping_add(low_bits).wrapping_add(n) as usize;
                let end = to + v66;
                if end > out.len() || from + v66 > input.len() {
                    return Err(Error("rtech literal run past the end"));
                }
                out[to..end].copy_from_slice(&input[from..from + v66]);
                in_pos += v66 as u64;
                out_pos += v66 as u64;
            } else {
                if to + 16 > out.len() || from + 16 > input.len() {
                    return Err(Error("rtech literal past the end"));
                }
                out[to..to + 16].copy_from_slice(&input[from..from + 16]);
                in_pos += n as u64;
                out_pos += n as u64;
            }
        } else {
            let v21 = (v19 & 0xf) as u32;
            d6c = 0;
            let t = (v21.wrapping_sub(31) >> 3) & 6;
            let e = ((v19 >> 4) & ((24 * ((v21.wrapping_sub(31) >> 3) & 2) as u64) >> 4)) as u32;
            let v22 = (((v19 as u32) as u64) >> t) & 0x3f;
            let v23: i32 = 1i32.wrapping_shl(v21 + e);
            let lut_bits = lut[v22 as usize + 1088] as u32;
            cur_bit += t + lut_bits + v21 + e;
            let v25 = 16i32.wrapping_mul(v23.wrapping_add((v23.wrapping_sub(1)) & ((v19 >> (t + lut_bits)) as i32)));
            v19 >>= t + lut_bits + v21 + e;
            let v26 = (v25 as u32).wrapping_add(lut[v22 as usize + 1024] as u32).wrapping_sub(16);
            let from = out_pos.wrapping_sub(v26 as u64) as usize;
            let to = out_pos as usize;
            if v26 as u64 > out_pos {
                return Err(Error("rtech match before the start"));
            }
            if v20 == 17 {
                let v41 = v19 >> 3;
                let v42 = (v19 & 7) as usize;
                let (mut v43, v44, v45);
                if v42 != 0 {
                    v43 = v41;
                    v44 = lut[v42 + 1232] as u32;
                    v45 = lut[v42 + 1240] as u32;
                } else {
                    cur_bit += 4;
                    let v46 = (v41 & 0xf) as usize;
                    v43 = v41 >> 4;
                    v44 = lut32(lut, 4 * v46 + 1152);
                    v45 = lut[v46 + 1216] as u32;
                    if cur_bit + v45 >= 61 {
                        v43 |= (input[in_pos as usize] as u64) << (61 - cur_bit);
                        in_pos += 1;
                        cur_bit -= 8;
                    }
                }
                cur_bit += v45 + 3;
                v19 = v43 >> v45;
                let v48 = ((v43 as u32) & ((1u32 << v45) - 1)) as u64 + v44 as u64 + 17;
                out_pos += v48;
                if to + v48 as usize + 8 > out.len() {
                    return Err(Error("rtech match past the end"));
                }
                if v26 < 8 {
                    let v50 = v48 - 13;
                    out_pos -= 13;
                    if v26 == 1 {
                        let b = out[from];
                        let mut k = 0;
                        while k < v50 as usize {
                            out[to + k..to + k + 8].fill(b);
                            k += 8;
                        }
                    } else {
                        for k in 0..v50 as usize {
                            out[to + k] = out[from + k];
                        }
                    }
                } else {
                    copy8(&mut out, from, to, v48 as usize);
                }
            } else {
                out_pos += v20 as u64;
                if to + 16 > out.len() {
                    return Err(Error("rtech match past the end"));
                }
                copy8(&mut out, from, to, 16);
            }
        }
        if in_pos < limit {
            cur_byte = v19;
            continue;
        }
        if out_pos == decomp_stream {
            if out_pos == decomp_size {
                break;
            }
            let v33 = input_inv & in_pos.wrapping_neg();
            v19 >>= 1;
            cur_bit += 1;
            if header_off > v33 {
                in_pos += v33;
                if in_pos > q70 {
                    q70 = input_inv.wrapping_add(q70).wrapping_add(1);
                }
            }
            let v35 = in_pos;
            in_pos += header_off;
            let mut v36 = out_pos + output_inv + 1;
            let v37 = rd64(input, v35) & ((1u64 << (8 * header_off)).wrapping_sub(1));
            size_needed += v37;
            comp_stream += v37;
            if v36 >= decomp_size {
                v36 = decomp_size;
                comp_stream += header_off;
            }
            decomp_stream = v36;
            if !(in_len >= size_needed) {
                return Err(Error("rtech stream runs past the file"));
            }
        }
        if in_pos >= q70 {
            in_pos = !input_inv & (in_pos + 7);
            q70 = q70.wrapping_add(input_inv).wrapping_add(1);
        }
        limit = if comp_stream < q70 { comp_stream } else { q70 };
        cur_byte = v19;
        if out_pos as usize > decompressed || in_pos > in_len + 16 {
            return Err(Error("rtech output past the expected size"));
        }
    }
    out.truncate(decompressed);
    Ok(out)
}
