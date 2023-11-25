use std::io::BufReader;
use std::rc::Rc;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io;

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

#[derive(Clone)]
enum IteratorIngredient {
    Data(Vec<u8>),
    Reference(Rc<Vec<u8>>)
}

#[derive(Clone)]
pub struct FileRecipeIterator {
    ingredient_iterator: <Vec<IteratorIngredient> as IntoIterator>::IntoIter,
    data: Vec<u8>,
    data_index: usize // i feel like i could use an iterator, but im done fighting the borrow
                      // checker
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
        fn get_n(file: &mut File, n: u64) -> io::Result<Option<Vec<u8>>> {
            let mut result = Vec::with_capacity(n as usize);

            file.by_ref().take(n).read_to_end(&mut result)?;

            if result.len() > 0 {
                return Ok(Some(result))
            } else {
                return Ok(None);
            }
        }

        let mut chunks: HashMap<Adler32Hash, Vec<(md5::Digest, u64)>> = HashMap::new();

        let mut n = 0;

        while let Some(data) = get_n(file, self.chunk_size)? {
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

        // this buffer contains all the data before we pushed a new ingredient to `recipe`.
        // Generally, this buffer will be equal to or larger than file_digest.chunk_size, unless a
        // "reload" step reached the EOF, in which case we are on the last iteration of the loop.
        let mut buffer = vec![];

        if 0 == dest_file.by_ref().take(file_digest.chunk_size).read_to_end(&mut buffer)? {
            // if the first load is empty, then that means there's no data in the dest_file,
            // therefore the file is empty, so no instructions are needed in the recipe
            return Ok(FileRecipe { recipe: vec![], chunk_size: file_digest.chunk_size });
        } 

        let mut rolling_hash = RollingAdler32::from_buffer(&buffer);

        let mut reader = BufReader::new(dest_file);

        loop {
            if { // this block returns ! if there's a chunk match or true
                match file_digest.chunks.get(&Adler32Hash(rolling_hash.hash())) {
                    Some(v) => { // an adler hash matched!
                        // double check with the md5 hash
                        if let Some(pair) = get_pair(&v, md5::compute(
                                buffer.iter().rev()
                                .take(file_digest.chunk_size as usize)
                                .rev()
                                .map(|x| *x).collect::<Vec<u8>>())) 
                        {
                            let excess = buffer
                                .iter().rev()
                                .skip(file_digest.chunk_size as usize)
                                .rev()
                                .map(|x| *x).collect::<Vec<u8>>();
                            if excess.len() > 0 { // dump the excess
                                recipe.push(FileIngredient::Data(excess));
                            } 
                            // push the new reference
                            recipe.push(FileIngredient::Reference(pair.1));

                            // reset the buffer
                            buffer.clear();

                            // read data into the hash buffer. if the buffer is empty, then there's
                            // no more file
                            if reader.by_ref().take(file_digest.chunk_size).read_to_end(&mut buffer)? == 0 {
                                break;
                            }
                            // reload the hash, since we cleared out the hash buffer
                            rolling_hash = RollingAdler32::from_buffer(&buffer);

                            continue
                        } else { // no md5 = no match
                            true
                        }
                    }
                    None => true // no adler = no match
                }
            } { // there was no match with the chunk :(
                if buffer.len() < file_digest.chunk_size as usize { // trust me, we were at the EOF
                    recipe.push(FileIngredient::Data(buffer));
                    break;
                }
                
                // pull the last seen byte out of the rolling hash window
                let dead_byte = buffer.get(buffer.len() - file_digest.chunk_size as usize)
                    .unwrap_or(&69); // if dead_byte is none, that means the next branch will fail
                                     // anyways, so it doesn't matter what the default value is
                rolling_hash.remove((file_digest.chunk_size - 1) as usize, *dead_byte);

                if let Some(byte) = reader.by_ref().bytes().next() { // get another byte
                    let byte = byte?;
                    rolling_hash.update(byte); // add it to the hash
                    buffer.push(byte); // and to the hash buffer
                } else { // if we can't get another byte, then there's nothing more in the file
                    recipe.push(FileIngredient::Data(buffer));
                    break;
                }
            }
        }

        Ok(FileRecipe { recipe, chunk_size: file_digest.chunk_size })
    }

    pub fn get_data(self, src_file: &mut File) -> io::Result<FileRecipeIterator> {
        let mut references: HashMap<u64, Rc<Vec<u8>>> = HashMap::new();
        let mut ingredients = vec![];

        for x in self.recipe {
            ingredients.push(
                match x {
                    FileIngredient::Data(v) => IteratorIngredient::Data(v),
                    FileIngredient::Reference(index) => {
                        if let Some(rc) = references.get(&index) {
                            IteratorIngredient::Reference(rc.clone())
                        } else {
                            let mut buf = vec![];

                            src_file.seek(io::SeekFrom::Start(index * self.chunk_size))?;
                            src_file.by_ref().take(self.chunk_size).read_to_end(&mut buf)?;

                            references.insert(index, Rc::new(buf));
                            IteratorIngredient::Reference(references.get(&index).unwrap().clone())
                        }
                    }
                }
            )
        }

        Ok(FileRecipeIterator {
            ingredient_iterator: ingredients.into_iter(),
            data: vec![],
            data_index: 0
        })
    }
}

impl Iterator for FileRecipeIterator {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(byte) = self.data.get(self.data_index) {
            self.data_index += 1;
            Some(*byte)
        } else {
            self.data_index = 0;
            match self.ingredient_iterator.next() {
                None => None,
                Some(IteratorIngredient::Data(v)) => {
                    self.data = v.clone();
                    self.next()
                }
                Some(IteratorIngredient::Reference(rc)) => {
                    self.data = rc.to_vec();
                    self.next()
                }
            }
        }
    }
}
