use std::cmp::Ordering;

/// Case-insensitive "natural" order: digit runs compare numerically, so
/// `DSC_2.NEF` sorts before `DSC_10.NEF`, as photographers expect.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = take_digits(&mut a);
                let nb = take_digits(&mut b);
                // Compare by value (ignoring leading zeros), then by length so that
                // "01" and "1" have a stable order.
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_sort_numerically() {
        let mut v = vec!["DSC_10.NEF", "DSC_2.NEF", "DSC_1.NEF", "dsc_3.nef"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["DSC_1.NEF", "DSC_2.NEF", "dsc_3.nef", "DSC_10.NEF"]);
    }

    #[test]
    fn leading_zeros_and_huge_numbers() {
        assert_eq!(natural_cmp("IMG_0009", "IMG_10"), Ordering::Less);
        assert_eq!(
            natural_cmp("a99999999999999999999999", "a100000000000000000000000"),
            Ordering::Less
        );
        assert_ne!(
            natural_cmp("x01", "x1"),
            Ordering::Equal,
            "stable, total order"
        );
    }

    #[test]
    fn prefixes_and_case() {
        assert_eq!(natural_cmp("Trip", "trip 2"), Ordering::Less);
        assert_eq!(natural_cmp("b", "A"), Ordering::Greater);
    }
}
