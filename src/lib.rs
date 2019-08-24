// Enables the use of Weak::strong_count() and Weak::weak_count().
#![feature(weak_counts)]
#![allow(clippy::new_without_default)]
#![feature(test)]
extern crate test;

#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use std::time::Instant;

pub mod base_letter_trie;
pub use base_letter_trie::BaseLetterTrie;
pub mod no_parent_letter_trie;
pub use no_parent_letter_trie::NoParentLetterTrie;
pub mod util;
pub use util::*;

const FILENAME_SMALL_SORTED: &str = "words_9_sorted.txt";
const FILENAME_SMALL_UNSORTED: &str = "words_9_unsorted.txt";
const FILENAME_MEDIUM_SORTED: &str = "words_10_000_sorted.txt";
const FILENAME_MEDIUM_UNSORTED: &str = "words_10_000_unsorted.txt";
const FILENAME_LARGE_SORTED: &str = "words_584_983_sorted.txt";
const FILENAME_LARGE_UNSORTED: &str = "words_584_983_unsorted.txt";
const FILENAME_GOOD_WORDS: &str = "test_good_words.txt";
const FILENAME_NON_WORDS: &str = "test_non_words.txt";
const USE_CHAR_GET_COUNTER: bool = false;

// const DEBUG_TREE_MAX_DEPTH: usize = 3;
// const DEBUG_TREE_MAX_CHILDREN: usize = 3;
const DEBUG_TREE_MAX_DEPTH: usize = 1000;
const DEBUG_TREE_MAX_CHILDREN: usize = 1000;

const LABEL_STEP_OVERALL: &str = "overall load";
const LABEL_STEP_READ_FILE: &str = "read file";
const LABEL_STEP_MAKE_VECTOR: &str = "make_vector";
const LABEL_STEP_SORT_VECTOR: &str = "sort_vector";
const LABEL_STEP_READ_AND_VECTOR: &str = "make vector from file";
const LABEL_STEP_LOAD_FROM_VEC: &str = "load from vector";

/// A [letter trie]: https://www.geeksforgeeks.org/trie-insert-and-search/ with implementations that use different
/// approaches for parent and child links but otherwise work the same.
pub trait LetterTrie {
    /// Create a trie from a text file containing one word per line. The words may be upper- or lowercase and
    /// blank lines and whitespace before or after the words will be ignored. Duplicate words will also be
    /// ignored.
    fn from_file(filename: &str, is_sorted: bool, load_method: &LoadMethod) -> Self;

    fn from_file_test(
        filename: &str,
        is_sorted: bool,
        load_method: &LoadMethod,
        opt: &DisplayDetailOptions,
    ) -> Self;

    fn find(&self, prefix: &str) -> Option<FixedNode>;

    fn to_fixed_node(&self) -> FixedNode;

    fn print_root(&self) {
        println!("{:?}", self.to_fixed_node());
    }

    fn print_root_alt(&self) {
        println!("{:#?}", self.to_fixed_node());
    }
}

/// Enum used to choose the collection of words to load in the letter trie. Whether the words are sorted in the
/// collection may affect the speed of loading the trie depending on the chosen LoadMethod but the resulting trie
/// will be identical either way.
#[derive(Debug)]
pub enum Dataset {
    /// Small file with nine sorted English words.
    TestSmallSorted,
    /// Small file with nine unsorted English words.
    TestSmallUnsorted,
    /// Medium file with 10,000 sorted non-English words.
    TestMediumSorted,
    /// Medium file with 10,000 unsorted non-English words.
    TestMediumUnsorted,
    /// Large file with 584,983 sorted non-English words.
    TestLargeSorted,
    /// Large file with 584,983 unsorted non-English words.
    TestLargeUnsorted,
}

impl Dataset {
    /// Get the path to a file with a set of words for testing.
    ///
    /// # Examples
    ///
    /// Get the path to a file that has 10,000 words.
    ///
    /// ```rust
    /// let filename = letter_trie::Dataset::TestMediumSorted.filename();
    /// ```
    pub fn filename(&self) -> &str {
        match self {
            Dataset::TestSmallSorted => FILENAME_SMALL_SORTED,
            Dataset::TestSmallUnsorted => FILENAME_SMALL_UNSORTED,
            Dataset::TestMediumSorted => FILENAME_MEDIUM_SORTED,
            Dataset::TestMediumUnsorted => FILENAME_MEDIUM_UNSORTED,
            Dataset::TestLargeSorted => FILENAME_LARGE_SORTED,
            Dataset::TestLargeUnsorted => FILENAME_LARGE_UNSORTED,
        }
    }

    /// Returns true if the dataset is already in alphabetical order.
    ///
    /// # Examples
    ///
    /// Get the path to a file that has 10,000 words.
    ///
    /// ```rust
    /// let is_sorted = letter_trie::Dataset::TestLargeUnsorted.is_sorted();
    /// assert_eq!(false, is_sorted);
    /// ```
    pub fn is_sorted(&self) -> bool {
        match self {
            Dataset::TestSmallSorted | Dataset::TestMediumSorted | Dataset::TestLargeSorted => true,
            Dataset::TestSmallUnsorted
            | Dataset::TestMediumUnsorted
            | Dataset::TestLargeUnsorted => false,
        }
    }
}

/// Enum used to choose between different implementations of LetterTrie.
#[derive(Debug)]
pub enum LetterTrieType {
    /// The baseline implementation using Rc<RefCell<Node>> for child links and Weak<RefCell<Node>> for parent links.
    Base,
    /// A stripped-down implementation with no parent links and with direct ownership of child nodes.
    NoParent,
}

/// The method the LetterTrie will use to load words from a text file.
#[derive(Debug, PartialEq)]
pub enum LoadMethod {
    /// Read the whole file into memory, create a vector of words, then fill the trie.
    ReadVecFill,
    /// Read the file into a vector in one step, then fill the tree.
    VecFill,
    /// Build the tree while reading lines from the file.
    Continuous,
    /// Read lines from the file, and as soon as all of the words for each starting letter have been read spawn affect
    /// thread to build a trie for that starting letter while continuing to read from the file in the first thread.
    /// As each thread finishes building its trie, merge that trie into the main trie.
    ContinuousParallel,
}

/// Options for the amount of detail to display while building a trie.
pub struct DisplayDetailOptions {
    /// If true, print the elapsed time for the whole trie build including reading the file.
    pub print_overall_time: bool,
    /// If true, print the elapsed time for each step. The particular steps depend on the chosen LoadMethod.
    pub print_step_time: bool,
    /// The amount of debugging information to print about the trie after it's been built:
    /// - 0: Print nothing
    /// - 1: Print a single line for the trie, the equivalent of `println!("{:?}", trie.to_fixed_node());`.
    /// - 2: Print a multiple lines for the trie, the equivalent of `println!("{:#?}", trie.to_fixed_node());`.
    pub object_detail_level: usize,
    /// The label to be displayed with any debugging information. One easy way to create this string is with a
    /// call to `get_test_label()`.
    pub label: String,
}

impl DisplayDetailOptions {
    pub fn make_no_display() -> Self {
        Self {
            print_overall_time: false,
            print_step_time: false,
            object_detail_level: 0,
            label: "".to_owned(),
        }
    }

    pub fn make_overall_time(
        dataset: &Dataset,
        load_method: &LoadMethod,
        char_trie_type: &LetterTrieType,
    ) -> Self {
        Self {
            print_overall_time: true,
            print_step_time: false,
            object_detail_level: 0,
            label: get_test_label(&dataset, &load_method, &char_trie_type),
        }
    }

    pub fn make_moderate(
        dataset: &Dataset,
        load_method: &LoadMethod,
        char_trie_type: &LetterTrieType,
    ) -> Self {
        Self {
            print_overall_time: true,
            print_step_time: true,
            object_detail_level: match dataset {
                Dataset::TestSmallSorted | Dataset::TestSmallUnsorted => 2,
                _ => 1,
            },
            label: get_test_label(&dataset, &load_method, &char_trie_type),
        }
    }
}

pub fn get_test_label(
    dataset: &Dataset,
    load_method: &LoadMethod,
    char_trie_type: &LetterTrieType,
) -> String {
    format!("{:?}; {:?}; {:?}", dataset, load_method, char_trie_type).to_owned()
}

#[derive(Debug, PartialEq)]
pub struct FixedNode {
    c: char,
    prefix: String,
    depth: usize,
    is_word: bool,
    child_count: usize,
    node_count: usize,
    word_count: usize,
    height: usize,
}

lazy_static! {
    static ref CHAR_GET_COUNTER: Mutex<CharGetCounter> = Mutex::new(CharGetCounter {
        hit_count: 0,
        miss_count: 0
    });
}

#[derive(Debug)]
pub struct CharGetCounter {
    hit_count: usize,
    miss_count: usize,
}

impl CharGetCounter {
    pub fn reset() {
        let mut counter = CHAR_GET_COUNTER.lock().unwrap();
        counter.hit_count = 0;
        counter.miss_count = 0;
    }

    pub fn record(is_hit: bool) {
        let mut counter = CHAR_GET_COUNTER.lock().unwrap();
        if is_hit {
            counter.hit_count += 1;
        } else {
            counter.miss_count += 1;
        }
    }

    pub fn print() {
        let counter = CHAR_GET_COUNTER.lock().unwrap();
        let total_count = counter.hit_count + counter.miss_count;
        if total_count == 0 {
            println!("CharGetCounter: nothing recorded");
        } else {
            let hit_pct = counter.hit_count as f64 / total_count as f64;
            println!(
                "CharGetCounter: hit count = {}; miss count = {}, hit pct = {}",
                format_count(counter.hit_count),
                format_count(counter.miss_count),
                hit_pct
            );
        }
    }

    pub fn print_optional() {
        let total_count: usize;
        {
            // Lock the counter and get the total count in a separate scope so that the counter is unlocked
            // before we call Self::print(). If we didn't do this, we'd still have a lock on CHAR_GET_COUNTER
            // when calling Self::print(). That function would try to get a lock and wait forever.
            let counter = CHAR_GET_COUNTER.lock().unwrap();
            total_count = counter.hit_count + counter.miss_count;
        }
        if total_count > 0 {
            Self::print();
        }
    }
}

fn make_vec_char(filename: &str, opt: &DisplayDetailOptions) -> Vec<Vec<char>> {
    let start = Instant::now();
    let file = File::open(filename).unwrap();
    let mut v: Vec<Vec<char>> = vec![];
    for line in BufReader::new(file).lines() {
        let line = line.unwrap();
        let line = line.trim();
        if !line.is_empty() {
            let vec_char: Vec<char> = line.to_lowercase().chars().collect();
            v.push(vec_char);
        }
    }
    print_elapsed_from_start(
        opt.print_step_time,
        &opt.label,
        LABEL_STEP_READ_AND_VECTOR,
        start,
    );

    if opt.object_detail_level >= 1 {
        println!("\nWord count = {}", v.len());
    }

    v
}

pub fn words_from_file(filename: &str) -> Vec<String> {
    let file = File::open(filename).unwrap();
    let mut v: Vec<String> = vec![];
    for line in BufReader::new(file).lines() {
        let line = line.unwrap();
        let line = line.trim();
        if !line.is_empty() {
            v.push(line.to_string());
        }
    }
    v
}

pub fn good_words() -> Vec<String> {
    words_from_file(FILENAME_GOOD_WORDS)
}

pub fn non_words() -> Vec<String> {
    words_from_file(FILENAME_NON_WORDS)
}

pub fn large_dataset_words_hash_set() -> HashSet<String> {
    let mut hash_set = HashSet::new();
    for word in words_from_file(Dataset::TestLargeSorted.filename()) {
        hash_set.insert(word);
    }
    hash_set
}
