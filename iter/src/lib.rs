pub fn flatten<I>(iter:I)->Flatten<I>{
    return Flatten::new(iter);
}

pub struct Flatten<O>{
    outer:O
}

impl<O> Flatten<O> {
    fn new(iter:O)->Flatten<O>{

        let mut new_vec:Vec<T>;

        Flatten{outer:iter}
    }
}

impl<O> Iterator for Flatten<O>
where 
O:Iterator,
O::Item:IntoIterator
{
    type Item=<O::Item as IntoIterator>::Item;

    fn next(&mut self) -> Option<Self::Item>{
       
       println!("Heyy... HERE");    
       self.outer.next().and_then(|iter| iter.into_iter().next())

    }

}


#[test]
fn empty()
{
    assert_eq!(flatten(std::iter::empty::<Vec<()>>()).count(),0);
}
#[test]
fn one()
{
    assert_eq!(flatten(std::iter::once(vec!["a"])).count(),1);
}
#[test]
fn two()
{
    assert_eq!(flatten(vec![vec!["1"],vec!["2"]].into_iter()).count(),2);
}
#[test]
fn two_inner()
{
    assert_eq!(flatten(std::iter::once(vec!["a","b"])).count(),2);
}