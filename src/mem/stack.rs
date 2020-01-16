use bytecode::atom::*;
use bytecode::attribute::{Attribute, ExceptionHandler};
use bytecode::method::Method;
use bytecode::*;
use interpreter;
use mem::metaspace::Klass;
use mem::{Slot, WideSlot, LONG_NULL, NULL, PTR_SIZE};
use std::sync::Arc;

pub struct JavaStack {
    // TODO thread
    pub frames: Vec<JavaFrame>,
    pub max_stack_size: usize,
}

impl JavaStack {
    // TODO
    pub fn new() -> JavaStack {
        JavaStack {
            frames: Vec::<JavaFrame>::new(),
            max_stack_size: 0,
        }
    }

    pub fn has_next(&self, pc: usize) -> bool {
        match self.frames.last() {
            Some(ref frame) => pc < frame.code.len(),
            None => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn invoke(&mut self, mut frame: JavaFrame, pc: usize) {
        if !self.is_empty() {
            let (_, ref descriptor, access_flag) = &frame.current_method;
            let (params, _) = interpreter::resolve_method_descriptor(descriptor);
            let mut slots: usize = params
                .into_iter()
                .map(|p| match p.as_ref() {
                    "D" | "J" => 2,
                    _ => 1,
                })
                .sum();
            if access_flag & METHOD_ACC_STATIC != METHOD_ACC_STATIC {
                slots = slots + 1;
            }
            let current = self.frames.last_mut().expect("Won't happend");
            unsafe {
                current.operands_ptr = current.operands_ptr.sub(slots * PTR_SIZE);
                current
                    .operands_ptr
                    .copy_to(frame.locals[..].as_mut_ptr(), slots * PTR_SIZE);
            }
            frame.pc = pc;
        }
        self.frames.push(frame);
    }

    pub fn backtrack(&mut self) -> usize {
        let frame = self.frames.pop().expect("Illegal operands stack: ");
        if !self.is_empty() {
            let (_, ref descriptor, _) = &frame.current_method;
            let (_, ret) = interpreter::resolve_method_descriptor(descriptor);
            let slots: usize = match ret.as_ref() {
                "D" | "J" => 2,
                "V" => 0,
                _ => 1,
            };
            let current = self.frames.last_mut().expect("");
            unsafe {
                current
                    .operands_ptr
                    .copy_from(frame.operands_ptr.sub(slots * PTR_SIZE), slots * PTR_SIZE);
                current.operands_ptr = current.operands_ptr.add(slots * PTR_SIZE);
            }
        }
        frame.pc
    }

    pub fn get_code(&self, pc: usize) -> u8 {
        self.frames.last().expect("Illegal class file").code[pc]
    }

    pub fn load(&mut self, offset: usize, count: usize) {
        let current = self.frames.last_mut().expect("Illegal class file");
        unsafe {
            current.operands_ptr.copy_from(
                current.locals[offset * PTR_SIZE..].as_ptr(),
                count * PTR_SIZE,
            );
            current.operands_ptr = current.operands_ptr.add(count * PTR_SIZE);
        }
    }

    pub fn store(&mut self, offset: usize, count: usize) {
        let current = self.frames.last_mut().expect("Illegal class file");
        unsafe {
            current.operands_ptr = current.operands_ptr.sub(count * PTR_SIZE);
            current.operands_ptr.copy_to(
                current.locals[offset * PTR_SIZE..].as_mut_ptr(),
                count * PTR_SIZE,
            );
        }
    }

    pub fn get(&self, offset: usize) -> Slot {
        let mut data = NULL;
        let current = self.frames.last().expect("Illegal operands");
        &data[..].copy_from_slice(&current.locals[offset * PTR_SIZE..(offset + 1) * PTR_SIZE]);
        data
    }

    pub fn get_w(&self, offset: usize) -> WideSlot {
        let mut data = LONG_NULL;
        let current = self.frames.last().expect("Illegal operands");
        &data[..].copy_from_slice(&current.locals[offset * PTR_SIZE..(offset + 2) * PTR_SIZE]);
        data
    }

    pub fn set(&mut self, offset: usize, v: Slot) {
        let frame = self.frames.last_mut().expect("Illegal class file");
        &frame.locals[offset * PTR_SIZE..].copy_from_slice(&v[..]);
    }

    pub fn set_w(&mut self, offset: usize, v: WideSlot) {
        let frame = self.frames.last_mut().expect("Illegal class file");
        &frame.locals[offset * PTR_SIZE..].copy_from_slice(&v[..]);
    }

    pub fn push(&mut self, v: &[u8]) {
        let current = self.frames.last_mut().expect("Illegal class file");
        unsafe {
            current.operands_ptr.copy_from(v.as_ptr(), PTR_SIZE);
            current.operands_ptr = current.operands_ptr.add(PTR_SIZE);
        }
    }

    pub fn pop(&mut self) -> Slot {
        let mut data = NULL;
        let current = self.frames.last_mut().expect("Illegal operands");
        unsafe {
            current.operands_ptr = current.operands_ptr.sub(PTR_SIZE);
            current.operands_ptr.copy_to(data.as_mut_ptr(), PTR_SIZE);
        }
        data
    }

    pub fn pop_w(&mut self) -> WideSlot {
        let mut data = LONG_NULL;
        let current = self.frames.last_mut().expect("Illegal operands");
        unsafe {
            current.operands_ptr = current.operands_ptr.sub(PTR_SIZE * 2);
            current
                .operands_ptr
                .copy_to(data.as_mut_ptr(), PTR_SIZE * 2);
        }
        data
    }
}

pub struct JavaFrame {
    pub locals: Vec<u8>,
    operands: Vec<u8>,
    pub operands_ptr: *mut u8,
    pub klass: Arc<Klass>,
    pub code: Arc<Vec<u8>>,
    pub exception_handlers: Arc<Vec<ExceptionHandler>>,
    pub current_method: (String, String, U2),
    pub pc: usize,
}

impl JavaFrame {
    pub fn new(class: Arc<Klass>, method: Arc<Method>) -> JavaFrame {
        let code_attribute = method
            .get_code()
            .expect("abstract method or interface not allowed");
        if let Attribute::Code(stacks, locals, ref code, ref exception, _) = code_attribute {
            let locals = vec![0u8; PTR_SIZE * locals as usize];
            let mut operands = vec![0u8; PTR_SIZE * stacks as usize];
            let operands_ptr = operands.as_mut_ptr();
            return JavaFrame {
                locals: locals,
                operands: operands,
                operands_ptr: operands_ptr,
                klass: class,
                code: Arc::clone(code),
                exception_handlers: Arc::clone(exception),
                current_method: method.get_name_and_descriptor(),
                pc: 0,
            };
        }
        panic!("won't happend");
    }

    pub fn dump(&self, pc: usize) {
        println!("current class: {:?}", self.klass.bytecode.get_name());
        println!(
            "current method: {:?} {:?}",
            self.current_method.0, self.current_method.1
        );
        println!("locals: {:02x?}", self.locals);
        println!("stacks: {:02x?}", self.operands);
        println!("pc: {:?}", pc);
        println!("instructions: {:02x?}\n", &self.code);
    }
}

#[test]
pub fn test() {
    let mut v = vec![0u8; 16];
    let ptr = v.as_mut_ptr();
    let data = [1u8, 1, 1, 1];
    unsafe {
        ptr.copy_from((&data).as_ptr(), 4);
    }
    assert_eq!(unsafe { *ptr }, 1);
    assert_eq!(unsafe { *ptr.add(4) }, 0);
    assert_eq!(v[0], 1);
    assert_eq!(v[15], 0);
}
