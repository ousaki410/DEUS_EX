//! Array System - TArray equivalent
//!
//! Reference: UE1 `Core/Containers/Array.h`

use std::ops::{Index, IndexMut};
use super::*;

/// Dynamic array (TArray equivalent)
#[derive(Debug, Clone)]
pub struct Array<T> {
    data: Vec<T>,
    // In UE, arrays can have a fixed size allocator
    // For now, just use Vec
}

impl<T> Array<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
    
    pub fn with_capacity(cap: usize) -> Self {
        Self { data: Vec::with_capacity(cap) }
    }
    
    pub fn len(&self) -> usize { self.data.len() }
    pub fn is_empty(&self) -> bool { self.data.is_empty() }
    pub fn capacity(&self) -> usize { self.data.capacity() }
    
    pub fn push(&mut self, value: T) { self.data.push(value); }
    pub fn pop(&mut self) -> Option<T> { self.data.pop() }
    
    pub fn insert(&mut self, index: usize, value: T) { self.data.insert(index, value); }
    pub fn remove(&mut self, index: usize) -> T { self.data.remove(index) }
    
    pub fn clear(&mut self) { self.data.clear(); }
    
    pub fn get(&self, index: usize) -> Option<&T> { self.data.get(index) }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> { self.data.get_mut(index) }
    
    pub fn iter(&self) -> std::slice::Iter<'_, T> { self.data.iter() }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> { self.data.iter_mut() }
    
    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [T] { &mut self.data }
    
    pub fn resize(&mut self, new_len: usize, value: T) where T: Clone {
        self.data.resize(new_len, value);
    }
    
    pub fn reserve(&mut self, additional: usize) { self.data.reserve(additional); }
    pub fn shrink_to_fit(&mut self) { self.data.shrink_to_fit(); }
}

impl<T> Default for Array<T> {
    fn default() -> Self { Self::new() }
}

impl<T> Index<usize> for Array<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output { &self.data[index] }
}

impl<T> IndexMut<usize> for Array<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output { &mut self.data[index] }
}

impl<T> IntoIterator for Array<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter { self.data.into_iter() }
}

impl<'a, T> IntoIterator for &'a Array<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.data.iter() }
}

impl<'a, T> IntoIterator for &'a mut Array<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.data.iter_mut() }
}

/// Fixed-size array (for replication)
#[derive(Debug, Clone)]
pub struct FixedArray<T, const N: usize> {
    data: [Option<T>; N],
    len: usize,
}

impl<T, const N: usize> FixedArray<T, N> {
    pub fn new() -> Self {
        Self { data: std::array::from_fn(|_| None), len: 0 }
    }
    
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.len < N {
            self.data[self.len] = Some(value);
            self.len += 1;
            Ok(())
        } else {
            Err(value)
        }
    }
    
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn capacity(&self) -> usize { N }
    
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len { self.data[index].as_ref() } else { None }
    }
}
