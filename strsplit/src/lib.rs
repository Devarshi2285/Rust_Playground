#[derive(Debug)]
pub struct StrSplit<'a,'b> {
    remainder: Option<&'a str>,
    delimiter: &'b str,
}

impl<'a,'b> StrSplit<'a,'b> {
    pub fn new(stack: &'a str, delimiter: &'b str) -> Self {
        StrSplit {
            remainder: Some(stack),
            delimiter,
        }
    }
}

impl<'a,'b> Iterator for StrSplit<'a,'b> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.delimiter.is_empty() {
            return None;
        }
        if let Some(ref mut s) = self.remainder{ 
        if let Some(delimiter_index) = s.find(self.delimiter) {
            let until_remainder = &s[..delimiter_index];
            *s =
                &s[(delimiter_index + self.delimiter.len())..];

            Some(until_remainder)
            
        }
        else {
            self.remainder.take()
        }
        } else if !self.remainder?.is_empty() {
            let last = self.remainder;
            self.remainder = Some("");
            Some(last.unwrap())
        } else {
            None
        }
    }
}

fn until_char<'a>(s:&'a str ,c:&'a char)->&'a str{
    let d=&format!("{}",c);
    let result=StrSplit::new(s, d);
    let vec:Vec<&str>=result.collect();
    return  vec[0];
}

#[test]
fn until_char_test() {
    assert_eq!(until_char("hello world",&'o'), "hell");
}

#[test]
fn test1(){

    let stack="1 2 3 ";
    let str_split=StrSplit::new(stack, " ");

    let result:Vec<&str>=str_split.collect();

    assert_eq!(result,vec!["1" , "2" , "3" ,""]);
}