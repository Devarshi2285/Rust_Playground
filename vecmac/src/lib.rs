macro_rules! myvec{

    ()=>{
        Vec::new()
    };
    ($($element:expr),+)=>{{
        let mut l=Vec::new();
        $(l.push($element);)+
        l
    }};
    ($element:expr ; $repetation:expr)=>{{

         let mut l=Vec::new();
         
         for number in 1..=$repetation {
            l.push($element);
         }
         l

    }}

}

#[test]
fn no_item(){
    let x:Vec<u32>=myvec![];
    assert!(x.is_empty());    
}
#[test]
fn one_item(){
    let x=myvec![10];
    assert_eq!(x.len(),1);
    assert_eq!(x[0],10);

    let y=myvec!["Hi"];
    assert_eq!(y.len(),1);
    assert_eq!(y[0],"Hi");
      
}
#[test]
fn multiple_item(){
    let x=myvec![10,20];
    assert_eq!(x.len(),2);
    assert_eq!(x,[10,20]);

    let y=myvec!["Hi";3];
    assert_eq!(y.len(),3);
    assert_eq!(y,["Hi","Hi","Hi"]);
      
}