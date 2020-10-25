pub struct Transaction {
    pub total: u16,
    pub remain: u16,
}

impl Transaction {
    pub fn new(num_blocks: u16) -> Self {
        Self { total: num_blocks, remain: num_blocks }
    }
}
