pub mod cseq_db;
pub mod fasta_io;
pub mod shmmrutils;

#[cfg(test)]
mod tests {
    use crate::fasta_io::{reverse_complement, FastaReader};
    use crate::shmmrutils::{match_reads, DeltaPoint};
    use flate2::bufread::MultiGzDecoder;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read};

    use crate::cseq_db::{self, deltas_to_aln_segs, reconstruct_seq_from_aln_segs};
    use crate::cseq_db::{Fragment, KMERSIZE};

    pub fn load_seqs() -> HashMap<String, Vec<u8>> {
        let mut seqs = HashMap::<String, Vec<u8>>::new();
        let filepath = "test/test_data/test_seqs.fa";
        let file = File::open(filepath.to_string()).unwrap();
        let mut reader = BufReader::new(file);
        let mut is_gzfile = false;
        {
            let r = reader.by_ref();
            let mut buf = Vec::<u8>::new();
            let _ = r.take(2).read_to_end(&mut buf);
            if buf == [0x1F_u8, 0x8B_u8] {
                log::info!("input file detected as gz-compressed file",);
                is_gzfile = true;
            }
        }
        drop(reader);

        let file = File::open(&filepath).unwrap();
        let mut reader = BufReader::new(file);
        let gz_buf = &mut BufReader::new(MultiGzDecoder::new(&mut reader));

        let file = File::open(&filepath).unwrap();
        let reader = BufReader::new(file);
        let std_buf = &mut BufReader::new(reader);

        let fastx_buf: &mut dyn BufRead = if is_gzfile {
            drop(std_buf);
            gz_buf
        } else {
            drop(gz_buf);
            std_buf
        };

        let mut fastx_reader = FastaReader::new(fastx_buf, &filepath.to_string()).unwrap();
        while let Some(rec) = fastx_reader.next_rec() {
            let rec = rec.unwrap();
            let seqname = String::from_utf8_lossy(&rec.id).into_owned();
            seqs.insert(seqname, rec.seq.clone());
        }
        seqs
    }

    #[test]
    fn load_seq_test() {
        let seqs = load_seqs();
        let mut csdb = cseq_db::CompressedSeqDB::new("test/test_data/test_seqs.fa".to_string());
        let _ = csdb.load_seqs();
        //println!("test");
        for seq in csdb.seqs.iter() {
            //println!();
            //println!("{}", seq.name);
            let mut reconstruct_seq = <Vec<u8>>::new();
            let mut _p = 0;
            for frg_id in seq.seq_frags.iter() {
                //println!("{}:{}", frg_id, csdb.frags[*frg_id as usize]);
                match csdb.frags.get(*frg_id as usize).unwrap() {
                    Fragment::Prefix(b) => {
                        reconstruct_seq.extend_from_slice(&b[..]);
                        //println!("p: {} {}", p, p + b.len());
                        _p += b.len();
                    }
                    Fragment::Suffix(b) => {
                        reconstruct_seq.extend_from_slice(&b[..]);
                        //println!("p: {} {}", p, p + b.len());
                        _p += b.len();
                    }
                    Fragment::Internal(b) => {
                        reconstruct_seq.extend_from_slice(&b[KMERSIZE as usize..]);
                        //println!("p: {} {}", p, p + b.len());
                        _p += b.len();
                    }
                    Fragment::AlnSegments((frg_id, reverse, a)) => {
                        if let Fragment::Internal(base_seq) =
                            csdb.frags.get(*frg_id as usize).unwrap()
                        {
                            let mut bs = base_seq.clone();
                            if *reverse == true {
                                bs = reverse_complement(&bs);
                            }
                            let seq = cseq_db::reconstruct_seq_from_aln_segs(&bs, a);
                            reconstruct_seq.extend_from_slice(&seq[KMERSIZE as usize..]);
                            //println!("p: {} {}", p, p + seq.len());
                            _p += seq.len();
                        }
                    }
                }
            }
            let orig_seq = seqs.get(&seq.name).unwrap();
            if reconstruct_seq != *orig_seq {
                //println!("{}", seq.name);
                //println!("{:?}", reconstruct_seq);
                //println!("{:?}", orig_seq);
                for i in 0..reconstruct_seq.len() {
                    if orig_seq[i] != reconstruct_seq[i] {
                        println!("{} {} {} X", i, orig_seq[i], reconstruct_seq[i]);
                    } else {
                        println!("{} {} {}  ", i, orig_seq[i], reconstruct_seq[i]);
                    }
                }
            };
            assert_eq!(reconstruct_seq, *orig_seq);

            let shmmrs = seq.shmmrs.clone();
            let mut px: u128 = 0;
            for shmmr in shmmrs.into_iter() {
                let shmmr_pair = px << 64 | (shmmr.x >> 8) as u128;
                //println!("spr {:?}", shmmr_pair);
                if csdb.frag_map.contains_key(&shmmr_pair) {
                    for (fid, sid) in csdb.frag_map.get(&shmmr_pair).unwrap() {
                        println!("matches: {} {} {}", seq.id, fid, sid);
                    }
                }
                px = (shmmr.x >> 8) as u128;
            }
        }
    }

    #[test]
    fn reconstruct_test1() {
        let base_frg = "TATTTATATTTATTTATATATATTTATATATTTATATATATATTTATATATAAATAT"
            .as_bytes()
            .to_vec();
        let frg = "TTTTTATTTTTTTAATTAATTAATTATTTATTTATTTATTTATTTATTTATTTATTT"
            .as_bytes()
            .to_vec();
        //let frg = "TTATATTTATTTATATATATTTATATAGTTTATATATATATTTATATATAAATATATA".as_bytes().to_vec();
        let m = match_reads(&base_frg, &frg, true, 0.1, 0, 0, 32);
        if let Some(m) = m {
            let deltas: Vec<DeltaPoint> = m.deltas.unwrap();
            let aln_segs = deltas_to_aln_segs(&deltas, m.end0 as usize, m.end1 as usize, &frg);
            let re_seq = reconstruct_seq_from_aln_segs(&base_frg, &aln_segs);
            if frg != re_seq || true {
                println!("{} {}", String::from_utf8_lossy(&base_frg), base_frg.len());
                println!("{} {}", String::from_utf8_lossy(&frg), frg.len());
                println!("{} {} {} {}", m.bgn0, m.end0, m.bgn1, m.end1);
                println!("{:?}", deltas);
                println!(
                    "{}",
                    String::from_utf8_lossy(&reconstruct_seq_from_aln_segs(&base_frg, &aln_segs))
                );
                println!("{:?}", aln_segs);
            }
            assert_eq!(frg, reconstruct_seq_from_aln_segs(&base_frg, &aln_segs));
        }
    }

    #[test]
    fn reconstruct_test2() {
        let base_frg = "TATTTATATTTATTTATATATATTTATATATTTATATATATATTTATATATAAATAT"
            .as_bytes()
            .to_vec();
        let frg = "TTTTTTATTTTTTTAATTAATTAATTATTTATTTATTTATTTATTTATTTATTTATT"
            .as_bytes()
            .to_vec();
        //let frg = "TTATATTTATTTATATATATTTATATAGTTTATATATATATTTATATATAAATATATA".as_bytes().to_vec();
        let m = match_reads(&base_frg, &frg, true, 0.1, 0, 0, 32);
        if let Some(m) = m {
            let deltas: Vec<DeltaPoint> = m.deltas.unwrap();
            let aln_segs = deltas_to_aln_segs(&deltas, m.end0 as usize, m.end1 as usize, &frg);
            let re_seq = reconstruct_seq_from_aln_segs(&base_frg, &aln_segs);
            if frg != re_seq || true {
                println!("{} {}", String::from_utf8_lossy(&base_frg), base_frg.len());
                println!("{} {}", String::from_utf8_lossy(&frg), frg.len());
                println!("{} {} {} {}", m.bgn0, m.end0, m.bgn1, m.end1);
                println!("{:?}", deltas);
                println!(
                    "{}",
                    String::from_utf8_lossy(&reconstruct_seq_from_aln_segs(&base_frg, &aln_segs))
                );
                println!("{:?}", aln_segs);
            }
            assert_eq!(frg, reconstruct_seq_from_aln_segs(&base_frg, &aln_segs));
        }
    }
}
