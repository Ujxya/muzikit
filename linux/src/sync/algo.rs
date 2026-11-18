//! Eşitleme algoritması. Saf işlev: iki defterden aynı planı üretir;
//! telefon da aynısını hesaplar, sonuçlar birebir karşılaştırılır.
//! Bütün kararlar ilk iki deftere göre TEK SEFERDE verilir; MAX bir kez hesaplanır.

#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    L,
    R,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::L => Side::R,
            Side::R => Side::L,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub number: u32,
    pub sha256: String,
    pub added_at: String,
    pub added_by: String,
    pub name: String,
}

impl Item {
    fn key(&self) -> (&str, &str, &str) {
        (&self.added_at, &self.added_by, &self.sha256)
    }
}

/// `side` tarafına gönderilecek şarkı.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Transfer {
    pub to: Side,
    pub sha256: String,
    pub number: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rename {
    pub side: Side,
    pub sha256: String,
    pub from: u32,
    pub to: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mismatch {
    pub sha256: String,
    pub l: u32,
    pub r: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duplicate {
    pub side: Side,
    pub sha256: String,
    pub number: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub transfers: Vec<Transfer>,
    pub renames: Vec<Rename>,
    pub mismatch: Vec<Mismatch>,
    pub duplicates: Vec<Duplicate>,
}

impl Plan {
    /// Karşılaştırma için kanonik sıra.
    pub fn normalize(&mut self) {
        self.transfers.sort_by(|a, b| (a.to, a.number, &a.sha256).cmp(&(b.to, b.number, &b.sha256)));
        self.renames.sort_by(|a, b| (a.side, a.from, &a.sha256).cmp(&(b.side, b.from, &b.sha256)));
        self.mismatch.sort();
        self.duplicates.sort_by(|a, b| (a.side, a.number, &a.sha256).cmp(&(b.side, b.number, &b.sha256)));
    }

    pub fn is_empty(&self) -> bool {
        self.transfers.is_empty() && self.renames.is_empty()
    }
}

/// `NNNN - başlık.opus` → yeni numarayla aynı başlık.
pub fn renumbered(name: &str, number: u32) -> String {
    let rest = name.get(7..).unwrap_or("");
    format!("{number:04} - {rest}")
}

struct View<'a> {
    /// sha → esas kayıt (en küçük numara)
    canon: HashMap<&'a str, &'a Item>,
    /// numara → o numaradaki tüm dosyalar
    slots: HashMap<u32, Vec<&'a Item>>,
    dups: Vec<&'a Item>,
}

fn view(items: &[Item]) -> View<'_> {
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
    let mut canon = HashMap::new();
    let mut slots: HashMap<u32, Vec<&Item>> = HashMap::new();
    let mut dups = Vec::new();
    for it in sorted {
        slots.entry(it.number).or_default().push(it);
        if canon.contains_key(it.sha256.as_str()) {
            dups.push(it);
        } else {
            canon.insert(it.sha256.as_str(), it);
        }
    }
    View { canon, slots, dups }
}

pub fn compute(l: &[Item], r: &[Item]) -> Plan {
    let views = [view(l), view(r)];
    let v = |s: Side| &views[s as usize];
    let max = l.iter().chain(r).map(|i| i.number).max().unwrap_or(0);
    let mut plan = Plan::default();

    for s in [Side::L, Side::R] {
        for d in &v(s).dups {
            plan.duplicates.push(Duplicate { side: s, sha256: d.sha256.clone(), number: d.number });
        }
    }
    for (sha, li) in &v(Side::L).canon {
        if let Some(ri) = v(Side::R).canon.get(sha)
            && li.number != ri.number
        {
            plan.mismatch.push(Mismatch { sha256: sha.to_string(), l: li.number, r: ri.number });
        }
    }

    // Yeni numara bekleyenler: (kaynak taraf, şarkı, kayma mı)
    let mut pool: Vec<(Side, &Item, bool)> = Vec::new();
    let mut handled: HashSet<(Side, &str)> = HashSet::new();

    for s in [Side::L, Side::R] {
        let t = s.other();
        let mut mine: Vec<&&Item> = v(s).canon.values().collect();
        mine.sort_by_key(|i| (i.number, &i.sha256));
        for item in mine.into_iter().copied() {
            if v(t).canon.contains_key(item.sha256.as_str()) || handled.contains(&(s, item.sha256.as_str())) {
                continue;
            }
            let n = item.number;
            let holders = v(t).slots.get(&n).map(Vec::as_slice).unwrap_or(&[]);
            let own = v(s).slots.get(&n).map(Vec::as_slice).unwrap_or(&[]);
            match holders {
                [] => plan.transfers.push(Transfer {
                    to: t,
                    sha256: item.sha256.clone(),
                    number: n,
                    name: item.name.clone(),
                }),
                // Çakışma: iki tarafta tek başına duran, birbirinde olmayan iki yeni şarkı
                [x] if own.len() == 1
                    && v(t).canon.get(x.sha256.as_str()).is_some_and(|c| std::ptr::eq(*c, *x))
                    && !v(s).canon.contains_key(x.sha256.as_str()) =>
                {
                    handled.insert((t, x.sha256.as_str()));
                    let (keep, keep_side, shift, shift_side) =
                        if item.key() < x.key() { (item, s, *x, t) } else { (*x, t, item, s) };
                    plan.transfers.push(Transfer {
                        to: keep_side.other(),
                        sha256: keep.sha256.clone(),
                        number: n,
                        name: keep.name.clone(),
                    });
                    pool.push((shift_side, shift, true));
                }
                // Numarayı iki tarafta da var olan bir şarkı ya da bir kopya tutuyor: yeni numara
                _ => pool.push((s, item, false)),
            }
        }
    }

    pool.sort_by(|a, b| a.1.key().cmp(&b.1.key()));
    for (i, (s, item, shift)) in pool.into_iter().enumerate() {
        let num = max + 1 + i as u32;
        let name = renumbered(&item.name, num);
        if shift {
            plan.renames.push(Rename {
                side: s,
                sha256: item.sha256.clone(),
                from: item.number,
                to: num,
                name: name.clone(),
            });
        } else {
            let (l, r) = if s == Side::L { (item.number, num) } else { (num, item.number) };
            plan.mismatch.push(Mismatch { sha256: item.sha256.clone(), l, r });
        }
        plan.transfers.push(Transfer { to: s.other(), sha256: item.sha256.clone(), number: num, name });
    }

    plan.normalize();
    plan
}

/// Planı uyguladıktan sonra bir tarafın son durumu (numara → sha). Testler için.
#[cfg(test)]
pub fn final_state(items: &[Item], side: Side, plan: &Plan) -> BTreeMap<u32, Vec<String>> {
    let mut m: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for it in items {
        let num = plan
            .renames
            .iter()
            .find(|r| r.side == side && r.sha256 == it.sha256 && r.from == it.number)
            .map_or(it.number, |r| r.to);
        m.entry(num).or_default().push(it.sha256.clone());
    }
    for t in plan.transfers.iter().filter(|t| t.to == side) {
        m.entry(t.number).or_default().push(t.sha256.clone());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn items(v: &Value) -> Vec<Item> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let n = r[0].as_u64().unwrap() as u32;
                Item {
                    number: n,
                    sha256: r[1].as_str().unwrap().into(),
                    added_at: r[2].as_str().unwrap().into(),
                    added_by: r[3].as_str().unwrap().into(),
                    name: format!("{n:04} - {}.opus", r[1].as_str().unwrap()),
                }
            })
            .collect()
    }

    fn side(s: &str, swap: bool) -> Side {
        let s = if s == "L" { Side::L } else { Side::R };
        if swap { s.other() } else { s }
    }

    fn state(v: &Value) -> BTreeMap<u32, Vec<String>> {
        let mut m: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for (k, s) in v.as_object().unwrap() {
            m.entry(k.parse().unwrap()).or_default().push(s.as_str().unwrap().into());
        }
        m
    }

    fn sorted(mut m: BTreeMap<u32, Vec<String>>) -> BTreeMap<u32, Vec<String>> {
        m.values_mut().for_each(|v| v.sort());
        m
    }

    /// Ortak vektörler (sync-test-vectors.json), iki yönde de
    #[test]
    fn shared_vectors_both_directions() {
        let Some(doc) = crate::sync::test_vectors() else { return };
        for case in doc["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let e = &case["expect"];
            for swap in [false, true] {
                let (mut l, mut r) = (items(&case["L"]), items(&case["R"]));
                if swap {
                    std::mem::swap(&mut l, &mut r);
                }
                let plan = compute(&l, &r);
                let (el, er) = if swap { (&e["R"], &e["L"]) } else { (&e["L"], &e["R"]) };
                let ctx = format!("{name} (swap={swap}) plan={plan:#?}");
                // Son durum kopyalar dahil (beklenen haritada kopyanın numarası da var)
                assert_eq!(sorted(final_state(&l, Side::L, &plan)), sorted(state(el)), "L: {ctx}");
                assert_eq!(sorted(final_state(&r, Side::R, &plan)), sorted(state(er)), "R: {ctx}");

                let mut renames: Vec<(Side, String, u32, u32)> = e["renames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| {
                        (
                            side(x["side"].as_str().unwrap(), swap),
                            x["sha"].as_str().unwrap().into(),
                            x["from"].as_u64().unwrap() as u32,
                            x["to"].as_u64().unwrap() as u32,
                        )
                    })
                    .collect();
                renames.sort();
                let mut got: Vec<_> = plan.renames.iter().map(|x| (x.side, x.sha256.clone(), x.from, x.to)).collect();
                got.sort();
                assert_eq!(got, renames, "renames: {ctx}");

                let mut mm: Vec<Mismatch> = e["mismatch"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| {
                        let (a, b) = (x["L"].as_u64().unwrap() as u32, x["R"].as_u64().unwrap() as u32);
                        let (l, r) = if swap { (b, a) } else { (a, b) };
                        Mismatch { sha256: x["sha"].as_str().unwrap().into(), l, r }
                    })
                    .collect();
                mm.sort();
                assert_eq!(plan.mismatch, mm, "mismatch: {ctx}");

                let mut dd: Vec<(Side, String, u32)> = e["duplicates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| {
                        (side(x["side"].as_str().unwrap(), swap), x["sha"].as_str().unwrap().into(), x["number"].as_u64().unwrap() as u32)
                    })
                    .collect();
                dd.sort();
                let mut got: Vec<_> = plan.duplicates.iter().map(|d| (d.side, d.sha256.clone(), d.number)).collect();
                got.sort();
                assert_eq!(got, dd, "duplicates: {ctx}");
            }
        }
    }

    #[test]
    fn renumbered_keeps_title() {
        assert_eq!(renumbered("0817 - Şarkı [x].opus", 818), "0818 - Şarkı [x].opus");
        assert_eq!(renumbered("0817 - .opus", 9), "0009 - .opus");
    }
}
