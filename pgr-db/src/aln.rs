// use rayon::prelude::*;
use crate::seq_db::{query_fragment, ShmmrToFrags};
use crate::shmmrutils::ShmmrSpec;
use rustc_hash::FxHashMap;
use std::collections::HashSet;

pub type HitPair = ((u32, u32, u8), (u32, u32, u8)); //(bgn1, end1, orientation1),  (bgn2, end2, orientation2)

pub fn sparse_aln(
    sp_hits: &mut Vec<HitPair>,
    max_span: u32,
    penality: f32,
) -> Vec<(f32, Vec<HitPair>)> {
    // given a set of hits in the form of (bgn1, end1, orientation1),  (bgn2, end2, orientation2)
    // perform (banded) dynamic programmng to group them into list of hit chains
    sp_hits.sort_by(|a, b| a.0 .0.partial_cmp(&b.0 .0).unwrap());
    let mut v_s = FxHashMap::<HitPair, f32>::default(); // score for each vertex
    let mut best_pre_v = FxHashMap::<HitPair, Option<HitPair>>::default(); // look up for the best pre-vertex
    assert!(sp_hits.len() > 1);
    let first_hp = sp_hits[0];
    v_s.insert(first_hp, first_hp.0 .1 as f32 - first_hp.0 .0 as f32); // the score of the first node is just its length
    best_pre_v.insert(first_hp, None);

    (1..sp_hits.len()).into_iter().for_each(|i| {
        let hp = sp_hits[i];
        let mut best_v = Option::<HitPair>::None;
        let mut best_s = 0_f32;
        let mut j = i;
        let mut span_set = HashSet::<(u32, u32, u8)>::new();
        loop {
            if j == 0 {
                break;
            };
            j -= 1;

            let pre_hp = sp_hits[j];
            if pre_hp.0 == hp.0 {
                continue;
            }; // don't connect node with the same left coordinate
            span_set.insert(pre_hp.0);
            let p_s = v_s.get(&pre_hp).unwrap_or(&0_f32);
            let mut s: f32 = *p_s as f32 + (hp.0 .1 as f32 - hp.0 .0 as f32);

            if hp.0 .2 == hp.1 .2 {
                // same orientation
                s -= penality
                    * ((hp.0 .0 as f32 - pre_hp.0 .1 as f32).abs()
                        + (hp.1 .0 as f32 - pre_hp.1 .1 as f32).abs());
            } else {
                // oppsite orientation
                s -= penality
                    * ((hp.0 .0 as f32 - pre_hp.0 .1 as f32).abs()
                        + (hp.1 .1 as f32 - pre_hp.1 .0 as f32).abs());
            }

            if s > best_s {
                best_s = s;
                best_v = Some(pre_hp);
            }

            if span_set.len() >= max_span as usize {
                break;
            };
        }

        if best_s > 0_f32 {
            v_s.insert(hp, best_s);
            best_pre_v.insert(hp, best_v);
        } else {
            v_s.insert(hp, hp.0 .1 as f32 - hp.0 .0 as f32);
            best_pre_v.insert(hp, None);
        }
    });

    let mut unvisited_v = HashSet::<HitPair>::new();
    unvisited_v.extend(sp_hits.iter());
    let mut out = Vec::<(f32, Vec<HitPair>)>::new();
    while unvisited_v.len() > 0 {
        let mut best_s = 0_f32; // global best score
        let mut best_v: Option<HitPair> = None; // global best vertex
                                                // println!("DBG unvisit len; {}", unvisited_v.len());
        unvisited_v.iter().for_each(|hp| {
            let s = v_s.get(&hp).unwrap_or(&0_f32);
            if *s > best_s {
                best_s = *s;
                best_v = Some(*hp);
            }
        });
        let mut track = Vec::<HitPair>::new();
        let mut v = best_v;
        while v.is_some() {
            if let Some(hp) = v {
                if !unvisited_v.contains(&hp) {
                    break;
                };
                track.push(hp);
                v = *best_pre_v.get(&hp).unwrap_or(&None);
            }
        }
        if track.len() == 0 {
            break;
        };
        track.reverse();
        track.iter().for_each(|hp| {
            // let s = v_s.get(hp).unwrap_or(&0_f32);
            // println!("H {} {} {} {} {} {} {}", hp.0.0, hp.0.1, hp.0.2, hp.1.0, hp.1.1, hp.1.2, s );
            unvisited_v.remove(hp);
        });
        let bgn_s = v_s.get(&track[0]).unwrap_or(&0_f32);
        out.push((best_s - bgn_s, track));
    }
    out
}

pub fn query_fragment_to_hps(
    shmap: &ShmmrToFrags,
    frag: &Vec<u8>,
    shmmr_spec: &ShmmrSpec,
    penality: f32,
    max_repeat_count: Option<u32>,
) -> Vec<(u32, Vec<(f32, Vec<HitPair>)>)> {
    let r = query_fragment(shmap, frag, &shmmr_spec);
    // group by target seq_id
    let mut sp_count0 = FxHashMap::<(u64, u64), u32>::default();
    let mut sp_count1 = FxHashMap::<(u64, u64, u32), u32>::default();
    r.iter().for_each(|d| {
        let sp = d.0;
        let e = sp_count0.entry(sp).or_insert(0);
        *e += 1;
        d.2.iter().for_each(|v| {
            let key = (sp.0, sp.1, v.1);
            let e = sp_count1.entry(key).or_insert(0);
            *e += 1;
        })
    });

    let mut sid_to_hits = FxHashMap::<u32, Vec<((u32, u32, u8), (u32, u32, u8))>>::default();
    r.into_iter().for_each(|d| {
        let sp = d.0;
        if *sp_count0.get(&sp).unwrap_or(&0) > 8 {
            return;
        };
        let left_frag_coor = d.1;
        d.2.iter().for_each(|v| {
            match max_repeat_count {
                Some(max_repeat_count) => {
                    if *sp_count1.get(&(sp.0, sp.1, v.1)).unwrap_or(&0) > max_repeat_count {
                        return;
                    }
                }
                None => {
                    if *sp_count1.get(&(sp.0, sp.1, v.1)).unwrap_or(&0) > 8 {
                        return;
                    }
                }
            }

            let e = sid_to_hits.entry(v.1).or_insert(vec![]);
            let right_frag_coor = (v.2, v.3, v.4);
            e.push((left_frag_coor, right_frag_coor));
        });
    });

    let out = sid_to_hits
        .into_iter()
        .filter(|(_sid, hps)| hps.len() > 1)
        .map(|(sid, mut hps)| (sid, sparse_aln(&mut hps, 8, penality)))
        .collect::<Vec<_>>();

    out
}

#[test]

fn sparse_aln_test() {
    use crate::aln::sparse_aln;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    let f = BufReader::new(File::open("./test/test_data/test_hits").unwrap());
    let mut hp = Vec::<HitPair>::new();
    f.lines().into_iter().for_each(|s| {
        if let Ok(s) = s {
            let s = s.split_ascii_whitespace();
            let out = s
                .into_iter()
                .map(|s| s.parse::<u32>().unwrap())
                .collect::<Vec<u32>>();
            assert_eq!(out.len(), 6);
            hp.push((
                (out[0], out[1], out[2] as u8),
                (out[3], out[4], out[5] as u8),
            ));
        }
    });

    let out = sparse_aln(&mut hp, 8, 0.5_f32);
    out.iter().for_each(|(s, v)| println!("{} {}", s, v.len()));
}
