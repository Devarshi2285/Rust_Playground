use my_macro::MySerialize;
use my_macro_2::MyDeSerialize;

trait MySerialize {
    fn serialize(&self) -> String;
}
trait MyDeSerialize {
    fn mydeserialize(data: &String) -> User;
}
#[derive(MySerialize, MyDeSerialize, Debug)]
struct User {
    username: String,
    age: u32,
    year: u32,
}

fn main() {
    let user = User {
        username: "John".to_string(),
        age: 30,
        year: 2005,
    };
    let userdata = user.serialize();
    println!("{}", &userdata);
    let user2 = User::mydeserialize(&userdata);
    println!("{:?}", user2);
}
