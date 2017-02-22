#[cfg(test)]
use quickcheck::TestResult;

pub fn nanoseconds_to_milliseconds(nanoseconds: u64) -> f64 {
    nanoseconds as f64 / 1_000_000_f64
}

/// Splits the given number such that no fractions are wasted and
/// the number is distributed evenly.
/// Example: Splitting 10 in 4 pieces gives [2,2,3,3]
pub fn split_number(to_split: usize, in_pieces: usize) -> Vec<usize> {
    if in_pieces < 1 {
        panic!("in_pieces under 1")
    }
    let each_must_have = to_split / in_pieces;
    let remainder = to_split % in_pieces;

    // the first pieces will be filled with each_must_have + remainder,
    // until remainder is 0
    let mut vec = vec!(each_must_have; in_pieces);
    for x in vec.iter_mut().take(remainder) {
        *x += 1
    }
    vec
}

#[test]
fn split_number1() {
    assert_eq!(split_number(10, 4), vec![3, 3, 2, 2]);
}

#[test]
fn split_number2() {
    assert_eq!(split_number(8, 4), vec![2, 2, 2, 2]);
}

#[test]
fn split_number3() {
    assert_eq!(split_number(7, 4), vec![2, 2, 2, 1]);
}

#[cfg(test)]
quickcheck! {
    fn split_number_sum_same(to_split: usize, in_pieces: usize) -> TestResult {
        if in_pieces < 1 {
            return TestResult::discard()
        }
        TestResult::from_bool(to_split == split_number(to_split, in_pieces).iter().sum())
    }

    fn split_number_pieces_returned(to_split: usize, in_pieces: usize) -> TestResult {
        if in_pieces < 1 {
            return TestResult::discard()
        }
        TestResult::from_bool(in_pieces == split_number(to_split, in_pieces).len())
    }
}
