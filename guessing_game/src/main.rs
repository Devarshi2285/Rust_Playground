use std::{cmp::Ordering, io};
use rand::Rng;


fn main() {
    println!("Guess Game");
    println!("Enter you Guess");

    let number=rand::thread_rng().gen_range(1..=100);
     let mut guess= String::new();


    loop{
    io::stdin()
        .read_line(&mut guess)
        .expect("failed");

    let guess:u32=match guess.trim().parse(){
        Ok(num)=>num,
        Err(_)=>continue,
    };

    println!("Your Guess {guess}");

    // match guess.cmp(&number) {
    //     Ordering::Less=>println!("Less"),
    //     Ordering::Greater=>println!("Greater"),
    //     Ordering::Equal=>{
    //         println!("You win");
    //         break;

    //     },
    // }
}

}