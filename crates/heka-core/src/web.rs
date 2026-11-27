#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub host: &'static str,
    pub path: String,
    pub user_agent: String,
    pub accept: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait Web {
    fn get(&self, request: &Request) -> Result<Answer, String>;
}
