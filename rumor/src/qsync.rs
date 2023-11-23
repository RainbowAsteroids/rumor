use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io;
use std::io::Bytes;
use std::slice;

use md5;
use adler32::RollingAdler32;

pub struct FileDigestBuilder {
    chunk_size: u64
}

#[derive(PartialEq, Eq, Hash)]
struct Adler32Hash(u32);
pub struct FileDigest {
    chunk_size: u64,
    chunks: HashMap<Adler32Hash, Vec<(md5::Digest, u64)>>
}

#[derive(Debug)]
enum FileIngredient {
    Data(Vec<u8>),
    Reference(u64)
}

#[derive(Debug)]
pub struct FileRecipe {
    chunk_size: u64,
    recipe: Vec<FileIngredient>
}

pub struct FileRecipeIterator<'a> {
    chunk_size: u64,
    src_file: &'a mut File,
    instruction_iterator: slice::Iter<'a, FileIngredient>,
    data: Vec<u8>,
}

impl Adler32Hash {
    pub fn new<T: AsRef<[u8]>>(data: T) -> Self {
        Adler32Hash(RollingAdler32::from_buffer(data.as_ref()).hash())
    }
}

impl FileDigestBuilder {
    pub fn new() -> Self {
        FileDigestBuilder {
            chunk_size: 512
        }
    }

    pub fn chunk_size(self, chunk_size: u64) -> Self {
        FileDigestBuilder { chunk_size, ..self }
    }  

    pub fn build(self, file: &mut File) -> io::Result<FileDigest> {
        // TODO: use Read.by_ref()?
        fn get_n<R: Read>(iter: &mut Bytes<R>, mut n: u64) -> io::Result<Option<Vec<u8>>> {
            let mut result = Vec::with_capacity(n.try_into().unwrap());

            for item in iter {
                result.push(item?);
                n -= 1;
                if n <= 0 { break; }
            }

            if result.len() > 0 {
                return Ok(Some(result))
            } else {
                return Ok(None);
            }
        }

        let mut chunks: HashMap<Adler32Hash, Vec<(md5::Digest, u64)>> = HashMap::new();

        let mut iter = file.bytes();

        let mut n = 0;

        while let Some(data) = get_n(&mut iter, self.chunk_size)? {
            let adler = Adler32Hash::new(&data);
            let md5_index_pair = (md5::compute(&data), n);

            if let Some(v) = chunks.get_mut(&adler) {
                v.push(md5_index_pair);
            } else {
                chunks.insert(
                    adler,
                    vec![md5_index_pair]
                );
            }

            n += 1;
        }

        Ok(FileDigest {
            chunk_size: self.chunk_size,
            chunks
        })
    }
}

// File'a -> FileDigest'a
// File'b + FileDigest'a -> FileRecipe'ab
// File'a + FileRecipe'ab -> Data (of File'b)

impl FileRecipe {
    pub fn new(dest_file: &mut File, file_digest: &FileDigest) -> io::Result<Self> {
        dest_file.seek(io::SeekFrom::Start(0))?; // reset to the start of the file

        fn get_pair(pairs: &[(md5::Digest, u64)], key: md5::Digest) -> Option<(md5::Digest, u64)> {
            for item in pairs {
                if key == item.0 {
                    return Some(*item);
                }
            }

            None
        }

        let mut recipe = vec![];
        let mut hash_buffer = vec![];

        if 0 == dest_file.by_ref().take(file_digest.chunk_size).read_to_end(&mut hash_buffer)? {
            // if the first load is empty, then that means there's no data in the dest_file,
            // therefore the file is empty, so no instructions are needed in the recipe
            return Ok(FileRecipe { recipe: vec![], chunk_size: file_digest.chunk_size });
        } 

        let mut rolling_hash = RollingAdler32::from_buffer(&hash_buffer);

        let mut arbitrary_buffer = vec![];
        loop {
            if { // this block returns ! if there's a chunk match or true
                match file_digest.chunks.get(&Adler32Hash(rolling_hash.hash())) {
                    Some(v) => { // an adler hash matched!
                        // double check with the md5 hash
                        if let Some(pair) = get_pair(&v, md5::compute(&hash_buffer)) {
                            if arbitrary_buffer.len() > 0 { // dump the arbitrary buffer
                                recipe.push(FileIngredient::Data(arbitrary_buffer));
                                arbitrary_buffer = vec![];
                            } 
                            // push the new reference
                            recipe.push(FileIngredient::Reference(pair.1));

                            // reset the hashed buffer
                            hash_buffer.clear();

                            // read data into the hash buffer. if the buffer is empty, then there's
                            // no more file
                            if dest_file.by_ref().take(file_digest.chunk_size).read_to_end(&mut hash_buffer)? == 0 {
                                break;
                            }
                            // reload the hash, since we cleared out the hash buffer
                            rolling_hash = RollingAdler32::from_buffer(&hash_buffer);

                            continue
                        } else { // no md5 = no match
                            true
                        }
                    }
                    None => true // no adler = no match
                }
            } { // there was no match with the chunk :(
                // pull the last seen byte out of the rolling hash window
                let dead_byte = hash_buffer.remove(0);
                rolling_hash.remove((file_digest.chunk_size - 1) as usize, dead_byte);
                arbitrary_buffer.push(dead_byte); // and add said byte into the arbitrary buffer

                if let Some(byte) = dest_file.by_ref().bytes().next() { // get another byte
                    let byte = byte?;
                    rolling_hash.update(byte); // add it to the hash
                    hash_buffer.push(byte); // and to the hash buffer
                } else { // if we can't get another byte, then there's nothing more in the file
                    arbitrary_buffer.extend_from_slice(&hash_buffer);
                    recipe.push(FileIngredient::Data(arbitrary_buffer));
                    break;
                }
            }
        }

        Ok(FileRecipe { recipe, chunk_size: file_digest.chunk_size })
    }

    pub fn get_data<'a>(&'a self, src_file: &'a mut File) -> FileRecipeIterator<'a> {
        FileRecipeIterator {
            chunk_size: self.chunk_size,
            src_file,
            instruction_iterator: self.recipe.iter(),
            data: vec![]
        }
    }
}

impl Iterator for FileRecipeIterator<'_> {
    type Item = io::Result<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(byte) = self.data.get(0) {
            let byte = *byte;
            self.data.remove(0);
            Some(Ok(byte))
       } else {
           match self.instruction_iterator.next() {
               None => None,
               Some(FileIngredient::Data(v)) => {
                   self.data = v.clone();
                   self.next()
               }
               Some(FileIngredient::Reference(index)) => {
                   if let Err(e) = self.src_file.seek(io::SeekFrom::Start(index * self.chunk_size)) {
                        return Some(Err(e));
                   }
                   
                   let mut buffer = vec![];
                   if let Err(e) = self.src_file.by_ref().take(self.chunk_size).read_to_end(&mut buffer) {
                       return Some(Err(e));
                   }

                   self.data = buffer;
                   self.next()
               }
           }
       }
    }
}
