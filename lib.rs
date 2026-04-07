pub trait Summary{
    fn summarize(&self)->String;
    fn read(&self)->();
}

pub struct Deva{
    pub title:String,
    pub text:String
}

impl Summary for Deva{
    fn summarize(&self)->String {
       return self.text.clone();
    }
    fn read(&self)->() {
        println!("{}",self.text)
    }
}