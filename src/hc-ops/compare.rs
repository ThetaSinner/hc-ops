use crate::cli::{CompareArgs, CompareCommands};
use crate::render::Render;
use anyhow::Context;
use base64::Engine;
use hc_ops::retrieve::SliceHash;
use nom::Parser;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{char, digit1, space1};
use nom::combinator::map_res;
use nom::multi::many1;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tabled::Tabled;

pub fn handle_compare_command(args: CompareArgs) -> anyhow::Result<()> {
    match args.command {
        CompareCommands::SliceHashes {
            our_file,
            their_file,
        } => compare_slice_hash_files(our_file, their_file)
            .map_err(|e| anyhow::anyhow!("Failed to compare slice hashes: {}", e))?,
    }

    Ok(())
}

fn compare_slice_hash_files(
    our_file: impl AsRef<Path>,
    their_file: impl AsRef<Path>,
) -> anyhow::Result<()> {
    #[derive(Tabled)]
    struct SliceHashDiffTable {
        pub dht_arc: String,
        pub slice_index: u64,
        pub diff: String,
    }

    let our_hashes = load_hash_file(our_file)?;
    let their_hashes = load_hash_file(their_file)?;

    let our_diffable = our_hashes
        .iter()
        .map(|oh| {
            (
                format!(
                    "{}-{}-{}",
                    oh.arc_start as u32, oh.arc_end as u32, oh.slice_index as u64
                ),
                oh,
            )
        })
        .collect::<HashMap<_, _>>();

    let our_keys = our_diffable.keys().collect::<HashSet<_>>();

    let their_diffable = their_hashes
        .iter()
        .map(|oh| {
            (
                format!(
                    "{}-{}-{}",
                    oh.arc_start as u32, oh.arc_end as u32, oh.slice_index as u64
                ),
                oh,
            )
        })
        .collect::<HashMap<_, _>>();

    let their_keys = &their_diffable.keys().collect::<HashSet<_>>();

    let our_extra = our_keys.difference(their_keys).collect::<Vec<_>>();

    let mut diff_table = Vec::new();

    for key in our_extra {
        if let Some(oh) = our_diffable.get(*key) {
            diff_table.push(SliceHashDiffTable {
                dht_arc: format!("{:?}", (oh.arc_start as u32)..(oh.arc_end as u32)),
                slice_index: oh.slice_index as u64,
                diff: "Only in our file".to_string(),
            });
        }
    }

    let their_extra = their_keys.difference(&our_keys).collect::<Vec<_>>();

    for key in their_extra {
        if let Some(oh) = their_diffable.get(*key) {
            diff_table.push(SliceHashDiffTable {
                dht_arc: format!("{:?}", (oh.arc_start as u32)..(oh.arc_end as u32)),
                slice_index: oh.slice_index as u64,
                diff: "Only in their file".to_string(),
            });
        }
    }

    let common_keys = our_keys.intersection(their_keys).collect::<Vec<_>>();

    for key in common_keys {
        if let (Some(oh1), Some(oh2)) = (our_diffable.get(*key), their_diffable.get(*key)) {
            if oh1.hash != oh2.hash {
                diff_table.push(SliceHashDiffTable {
                    dht_arc: format!("{:?}", (oh1.arc_start as u32)..(oh1.arc_end as u32)),
                    slice_index: oh1.slice_index as u64,
                    diff: format!(
                        "Different hashes: our hash = {}, their hash = {}",
                        base64::prelude::BASE64_STANDARD.encode(&oh1.hash),
                        base64::prelude::BASE64_STANDARD.encode(&oh2.hash)
                    ),
                });
            }
        }
    }

    if diff_table.is_empty() {
        println!("No differences found between the two files.");
    } else {
        diff_table.render(std::io::stdout())?
    }

    Ok(())
}

fn load_hash_file(path: impl AsRef<Path>) -> anyhow::Result<Vec<SliceHash>> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)
        .context("Failed to load input file")?
        .lines()
    {
        let Ok((_, (_, _, start, _, end, _, _, index, _, hash))) = (
            many1(alt((space1, tag("├"), tag("│"), tag("┤")))),
            tag::<_, _, nom::error::Error<_>>("Arc("),
            map_res(digit1, |s: &str| s.parse::<u32>()),
            tag(", "),
            map_res(digit1, |s: &str| s.parse::<u32>()),
            char(')'),
            many1(alt((space1, tag("├"), tag("│"), tag("┤")))),
            map_res(digit1, |s: &str| s.parse::<u64>()),
            many1(alt((space1, tag("├"), tag("│"), tag("┤")))),
            map_res(take_until(" "), |hash: &str| {
                base64::prelude::BASE64_STANDARD.decode(hash)
            }),
        )
            .parse(line)
        else {
            continue;
        };

        out.push(SliceHash {
            arc_start: start as i32,
            arc_end: end as i32,
            slice_index: index as i64,
            hash,
        });
    }

    Ok(out)
}
