use std::fmt::Error;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::RangeFull;

const PAGE_SIZE: usize = 4096;
const HEADER_SIZE: usize = 5;
const SLOT_SIZE: usize = 4;

//
// ---------------- USER ----------------
//

pub struct User<> {
    pub name: String,
    pub pass: String,
}

//
// ---------------- PAGER ----------------
//

pub struct Pager {
    file: File,
}

impl Pager {

    pub fn new() -> Result<Self, io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("db.bin")?;

        Ok(Self { file })
    }

    pub fn read_page(&mut self, page_id: u64) -> Result<Page, io::Error> {
        let offset = page_id * PAGE_SIZE as u64;

        let mut data = [0u8; PAGE_SIZE];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut data)?;

        Ok(Page { page_id, data })
    }

    pub fn write_page(&mut self, page: &Page) -> Result<(), io::Error> {
        let offset = page.page_id * PAGE_SIZE as u64;

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&page.data)?;
        // self.file.sync_all()?;

        Ok(())
    }

    pub fn initialize_if_empty(&mut self) -> Result<(), io::Error> {
        if self.file.metadata()?.len() == 0 {

            let dict_page = init_dict_page();
            self.write_page(&dict_page)?;

            let data_page = init_data_page(&1);
            self.write_page(&data_page)?;
        }

        Ok(())
    }

    pub fn read_from_pages_by_name(&mut self,name:&str)->Result<User, io::Error>{
        let dict_buff=self.read_page(0)?;
        let dict = parse_dict(&dict_buff.data);

        let currentPage=dict.curr_page;

       for pageid in 1..=currentPage{

            let mut page=self.read_page(pageid as u64)?;

            let user=page.read_user_by_name(&name);
            match user {
                Ok(user)=>{
                     return Ok(user);
                }
                Err(err)=>{
                    continue;
                }
            }
           
        }

        return Err(io::Error::new(ErrorKind::NotFound, "Can't find user"));

    }

}

//
// ---------------- PAGE ----------------
//

pub struct Page {
    pub page_id: u64,
    pub data: [u8; PAGE_SIZE],
}

impl Page {

    pub fn new(pageid: u64,data:[u8;4096])->Page{
        Page { page_id: pageid, data: data }
    }

    pub fn parse_header(&self) -> PageHeader {
        PageHeader {
            page_type: self.data[0],
            slot_count: read_u16(&self.data, 1),
            free_space_offset: read_u16(&self.data, 3),
        }
    }

    pub fn insert_user(&mut self, user: &User ,pager: &mut Pager) -> Result<(), io::Error> {

        let mut header = self.parse_header();

        let name_bytes = user.name.as_bytes();
        let pass_bytes = user.pass.as_bytes();

        let tuple_size = 4 + name_bytes.len() + pass_bytes.len();

        let slot_directory_end =
            HEADER_SIZE + header.slot_count as usize * SLOT_SIZE;

        let free_space =
            header.free_space_offset as usize - slot_directory_end;

        if tuple_size + SLOT_SIZE > free_space {
            
            return  Err(io::Error::new(ErrorKind::QuotaExceeded, "No space"));
            
        }

        let new_record_offset =
            header.free_space_offset as usize - tuple_size;

        // Write lengths
        write_u16(&mut self.data, new_record_offset, name_bytes.len() as u16);
        write_u16(&mut self.data, new_record_offset + 2, pass_bytes.len() as u16);

        // Write name
        self.data[new_record_offset + 4 ..
                  new_record_offset + 4 + name_bytes.len()]
            .copy_from_slice(name_bytes);

        // Write password
        self.data[new_record_offset + 4 + name_bytes.len() ..
                  new_record_offset + 4 + name_bytes.len() + pass_bytes.len()]
            .copy_from_slice(pass_bytes);

        // Write slot entry
        let slot_offset =
            HEADER_SIZE + header.slot_count as usize * SLOT_SIZE;

        write_u16(&mut self.data, slot_offset, new_record_offset as u16);
        write_u16(&mut self.data, slot_offset + 2, tuple_size as u16);

        // Update header
        header.slot_count += 1;
        header.free_space_offset = new_record_offset as u16;

        write_u8(&mut self.data, 0, header.page_type);
        write_u16(&mut self.data, 1, header.slot_count);
        write_u16(&mut self.data, 3, header.free_space_offset);

        Ok(())
    }

    pub fn read_user_by_name(&mut self , name:&str)->Result<User,io::Error>{

        let header=self.parse_header();
        let mut solts=header.slot_count;
        
        for n in 1..solts{
            let soltadd=HEADER_SIZE + (n as usize * SLOT_SIZE);
            let tuplefffset=read_u16(&self.data, soltadd ) as usize;
            let tuplesize=read_u16(&self.data, soltadd+2);

            let namelen=read_u16(&self.data,tuplefffset );
            let passlen=read_u16(&self.data,tuplefffset+2);
           

            let namefromdb = self.data[tuplefffset+4 as usize..tuplefffset+4+namelen as usize].to_vec();

            let pass=self.data[tuplefffset+4+namelen as usize..tuplefffset+4+namelen as usize+passlen as usize].to_vec();
            
            let name_str=String::from_utf8(namefromdb).unwrap();
            let pass_str=String::from_utf8(pass.to_vec()).unwrap();
            if name_str==name.to_string() {
                return  Ok(User{
                    name:name_str,
                    pass:pass_str
                });
            
            }    
        }
        
        return Err(io::Error::new(ErrorKind::NotFound, "Can't find user"));
        
    }
}

//
// ---------------- HEADER STRUCT ----------------
//

pub struct PageHeader {
    pub page_type: u8,
    pub slot_count: u16,
    pub free_space_offset: u16,
}

//
// ---------------- DICT ----------------
//

pub struct Dict {
    pub curr_page: u16,
}

fn parse_dict(buffer: &[u8; PAGE_SIZE]) -> Dict {
    Dict {
        curr_page: read_u16(buffer, 0),
    }
}

fn init_dict_page() -> Page {
    let mut data = [0u8; PAGE_SIZE];

    write_u16(&mut data, 0, 1); // current page = 1

    Page { page_id: 0, data }
}

//
// ---------------- DATA PAGE INIT ----------------
//

fn init_data_page(page_id: &u64) -> Page {
    let mut data = [0u8; PAGE_SIZE];

    write_u8(&mut data, 0, 1); // page_type
    write_u16(&mut data, 1, 0); // slot_count
    write_u16(&mut data, 3, PAGE_SIZE as u16); // free_space_offset

    Page { page_id:page_id.clone(), data }
}

//
// ---------------- ENGINE LAYER ----------------
//

pub fn insert_user(pager: &mut Pager, user: User) -> Result<(), io::Error> {


    loop {

    let mut dict_page = pager.read_page(0)?;
    let dict = parse_dict(&dict_page.data);

    let mut page = pager.read_page(dict.curr_page as u64)?;

    if page.insert_user(&user,pager).is_ok() {

        pager.write_page(&page)?;
        break;
    }

    // Page full → allocate new page

    let new_page_id = dict.curr_page as u64 + 1;

    let new_page = init_data_page(&new_page_id);
    pager.write_page(&new_page)?;

    // Update dict
    write_u16(&mut dict_page.data, 0, new_page_id as u16);
    pager.write_page(&dict_page)?;
}

    Ok(())
}

//
// ---------------- BYTE HELPERS ----------------
//

fn write_u8(buffer: &mut [u8], offset: usize, value: u8) {
    buffer[offset] = value;
}
fn read_u8(buffer: &[u8], offset: usize) -> u8 {
    let mut tmp = [0u8; 1];
    tmp.copy_from_slice(&buffer[offset..offset + 1]);
    u8::from_be_bytes(tmp)
}

fn write_u16(buffer: &mut [u8], offset: usize, value: u16) {
    buffer[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn read_u16(buffer: &[u8], offset: usize) -> u16 {
    let mut tmp = [0u8; 2];
    tmp.copy_from_slice(&buffer[offset..offset + 2]);
    u16::from_be_bytes(tmp)
}

//
// ---------------- TEST ----------------
//

#[test]
    fn full_flow_test() {

        let mut pager = Pager::new().unwrap();
        pager.initialize_if_empty().unwrap();

        for i in 1..1000{
        let s1="Dev";
        let s2=i.to_string();    
        insert_user(    
            &mut pager,
            User {
                name: (format!("{s1} {s2}")).to_string(),
                pass: "123".to_string(),
            },
        ).unwrap();

        }
    }
#[test]
fn check_user() {
    let user=Pager::new().unwrap().read_from_pages_by_name(&"Dev 999").unwrap();

    assert_eq!(user.name,"Dev 999".to_string());
    assert_eq!(user.pass,"123".to_string());
}