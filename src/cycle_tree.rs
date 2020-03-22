/// A CycleTree is a simple way to construct repeating structures with arbitrary
/// leaf nodes.  This is useful for things like workouts, where intervals are
/// repetitions, and then you might repeat sections of intervals, and then have
/// warmup and cooldowns.  A simple example of 10 minute warm up, 8 repeats of
/// 1min on 2min off, and then 10min cooldown:
///   Node(1,vec![
///     Leaf(10),
///     Node(5, vec![
///       Leaf(1),
///       Leaf(2),
///     ]),
///     Leaf(10),
///   ])
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum CycleTree<L> {
    Leaf(L),
    Node((usize, Vec<CycleTree<L>>)),
}

fn flatten<L: Copy>(cycle_tree: &CycleTree<L>, v: &mut Vec<L>) {
    match cycle_tree {
        CycleTree::Leaf(l) => v.push(*l),
        CycleTree::Node((c, ws)) => {
            for _ in 0..*c {
                for w in ws {
                    flatten(w, v);
                }
            }
        }
    }
}

// TODO: Make this more efficient, but notably gets the right interface in place
impl<L: Copy> IntoIterator for CycleTree<L> {
    type Item = L;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut v = Vec::new();
        flatten(&self, &mut v);
        v.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::CycleTree;

    #[test]
    fn simple() {
        let expected: Vec<u32> = vec![0, 1, 2];
        let result: Vec<u32> = CycleTree::Node((
            1,
            vec![CycleTree::Leaf(0), CycleTree::Leaf(1), CycleTree::Leaf(2)],
        ))
        .into_iter()
        .collect();
        assert_eq!(expected, result);
    }

    #[test]
    fn simple_cycle() {
        let expected: Vec<u32> = vec![0, 1, 2, 0, 1, 2];
        let result: Vec<u32> = CycleTree::Node((
            2,
            vec![CycleTree::Leaf(0), CycleTree::Leaf(1), CycleTree::Leaf(2)],
        ))
        .into_iter()
        .collect();
        assert_eq!(expected, result);
    }

    #[test]
    fn nested_cycle() {
        let expected: Vec<u32> = vec![0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 1, 2];
        let result: Vec<u32> = CycleTree::Node((
            2,
            vec![
                CycleTree::Node((2, vec![CycleTree::Leaf(0), CycleTree::Leaf(0)])),
                CycleTree::Leaf(1),
                CycleTree::Leaf(2),
            ],
        ))
        .into_iter()
        .collect();
        assert_eq!(expected, result);
    }
}
