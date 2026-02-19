// library for presenting sixel and kitty images

use crate::glob_vars::*;
use std::{
    collections::HashMap,
    io::{self, Read, Write},
};

// function to measure kitty image height
pub fn kitty_rows(s: &str) -> Option<usize> {
    // match: ESC_G ... (terminated by ST `ESC\` or ST 0x9c, or BEL 0x07)
    // captures the body (params[,;]data...), enabling parsing params before the first ;
    let re = regex::Regex::new(r"\x1b_G(?P<body>.*?)(?:\x1b\\|\x9c|\x07)").ok()?;
    let mut id_to_rows: HashMap<String, usize> = HashMap::new();
    let mut anon_images = 0usize;

    for caps in re.captures_iter(s) {
        let body = match caps.name("body") {
            Some(m) => m.as_str(),
            None => continue,
        };

        // params are before the first ; (then optional base64 data after ';')
        let params_str = body.split_once(';').map(|(p, _)| p).unwrap_or(body);

        let mut img_id: Option<String> = None;
        let mut rows: Option<usize> = None;

        for part in params_str.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                match k {
                    "i" => img_id = Some(v.to_string()),
                    "r" => rows = v.parse::<usize>().ok(),
                    _ => {}
                }
            }
        }

        if let Some(r) = rows {
            if let Some(id) = img_id {
                // for the same image id, keep the largest r seen
                id_to_rows
                    .entry(id)
                    .and_modify(|x| {
                        if r > *x {
                            *x = r
                        }
                    })
                    .or_insert(r);
            } else {
                // no explicit id — treat as its own image
                anon_images += r;
            }
        }
    }

    let sum_ids: usize = id_to_rows.values().copied().sum();
    let total = sum_ids + anon_images;
    if total > 0 { Some(total) } else { None }
}

// functions to measure sixel graphics height (raster row, not terminal row, string between ESC P and ST)
fn sixel_block_raster_rows(body: &str) -> Option<usize> {
    // sixel data starts after the first 'q'
    let idx = body.find('q')?;
    let data = &body[idx + 1..];
    if data.is_empty() {
        return Some(0);
    }
    // '-' advances to next sixel row; '$' is carriage return (same row)
    Some(1 + data.bytes().filter(|&b| b == b'-').count())
}

// sum of raster rows across all sixel images found in `s`.
// returns None if no sixel blocks are present.
pub fn sixel_rows(s: &str) -> Option<usize> {
    // match DCS ... ST: ESC P ... (terminated by ESC\ or 0x9C)
    let re = regex::Regex::new(r"(?s)\x1bP(?P<body>.*?)(?:\x1b\\|\x9c)").ok()?;
    let mut total: usize = 0;
    let mut found = false;

    for caps in re.captures_iter(s) {
        if let Some(body) = caps.name("body") {
            if let Some(rows) = sixel_block_raster_rows(body.as_str()) {
                total += rows;
                found = true;
            }
        }
    }

    if found { Some(total) } else { None }
}

// functions to get term cell height, just for converting sixel rows to terminal rows
pub fn term_cell_height_cached() -> std::io::Result<usize> {
    if let Some(h) = CELL_HEIGHT.get() {
        return Ok(*h);
    }
    let h = terminal_cell_height_px()?;
    CELL_HEIGHT.set(h).ok();
    Ok(h)
}

fn write_csi_and_read<'a>(
    out: &mut dyn Write,
    inp: &mut dyn Read,
    csi: &[u8],
    buf: &'a mut [u8],
) -> io::Result<&'a str> {
    out.write_all(csi)?;
    out.flush()?;
    let n = inp.read(buf)?;
    std::str::from_utf8(&buf[..n]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn parse_csi_numbers(s: &str, expected_leading: &str) -> Option<Vec<usize>> {
    let rest = s.strip_prefix("\x1b[")?;
    let rest = rest.strip_suffix('t')?;
    let mut it = rest.split(';');
    if it.next()? != expected_leading {
        return None;
    }
    let mut out = Vec::new();
    for p in it {
        out.push(p.parse().ok()?);
    }
    Some(out)
}

fn terminal_cell_height_px() -> io::Result<usize> {
    // use /dev/tty-style stdin/stdout, pure std I/O
    let mut out = io::stdout();
    let mut inp = io::stdin();

    // 1) try CSI 16 t : "report cell size in pixels" -> ESC [ 16 t?
    // many terminals reply as: ESC [ 6 ; <height_px> ; <width_px> t
    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[16t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "6") {
                if nums.len() >= 2 && nums[0] > 0 {
                    return Ok(nums[0]); // height in pixels per cell
                }
            }
        }
    }

    // 2) fallback: derive from window pixel height / rows
    //    CSI 14 t -> report window size in pixels: ESC [ 4 ; <height_px> ; <width_px> t
    //    CSI 18 t -> report text area size in characters: ESC [ 8 ; <rows> ; <cols> t
    let mut height_px: Option<usize> = None;
    let mut rows: Option<usize> = None;

    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[14t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "4") {
                if nums.len() >= 2 && nums[0] > 0 {
                    height_px = Some(nums[0]);
                }
            }
        }
    }
    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[18t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "8") {
                if nums.len() >= 2 && nums[0] > 0 {
                    rows = Some(nums[0]);
                }
            }
        }
    }

    if let (Some(hpx), Some(r)) = (height_px, rows) {
        if r > 0 {
            let per_cell = (hpx + r - 1) / r;
            return Ok(per_cell);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        "could not determine cell height in pixels (ANSI queries failed)",
    ))
}
