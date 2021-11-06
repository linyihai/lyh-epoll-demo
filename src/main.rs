// copied from mio: https://github.com/tokio-rs/mio
#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::io;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};


const HTTP_RESP: &[u8] = b"HTTP/1.1 200 OK
content-type: text/html
content-length: 5

Hello";


// EPOLLONESHOT 只想收到对应的事件一次，之后epoll就不再通知
// 所以下文又重新注册了事件
const READ_FLAGS:i32 = libc::EPOLLONESHOT | libc::EPOLLIN;
const WRITE_FLAGS:i32 = libc::EPOLLONESHOT | libc::EPOLLOUT;

fn epoll_create () -> io::Result<RawFd> {
    let fd = syscall!(epoll_create1(0))?;
    if let Ok(flags) = syscall!(fcntl(fd, libc::F_GETFD)) {
        let _ = syscall!(fcntl(fd, libc::F_SETFL, flags | libc::FD_CLOEXEC));
    }
    Ok(fd)
}

fn add_interst(epoll_fd: RawFd, fd: RawFd, mut event: libc::epoll_event) -> io::Result<()> {
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event))?;
    Ok(())
}


// EPOLL_CTL_MOD flags 表示修改已经加到epoll 队列中的fd。
fn modify_interest(epoll_fd: RawFd, fd: RawFd, mut event: libc::epoll_event) -> io::Result<()> {
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_MOD, fd, &mut event))?;
    Ok(())
}

// EPOLL_CTL_DEL 从epoll 队列移除掉fd
fn remove_interest(epoll_fd: RawFd, fd: RawFd) -> io::Result<()> {
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()))?;
    Ok(())
}


fn listener_read_event(key: u64) -> libc::epoll_event {
    libc::epoll_event {
        events: READ_FLAGS as u32,
        u64: key,
    }
}

fn listener_write_event(key: u64) -> libc::epoll_event {
    libc::epoll_event {
        events: WRITE_FLAGS as u32,
        u64: key,
    }
}

#[derive(Debug)]
pub struct RequestContext {
    pub stream: TcpStream,
    pub content_length: usize,
    pub buf: Vec<u8>,
}

// read_cb 从socket stream 读取数据。每次读4096字节，如果没读完，就通过EPOLL_FLAG_MOD
// write_cb 发送完响应后就从epoll 队列移除掉fd，并关闭fd
impl RequestContext {
    fn new(stream: TcpStream) -> RequestContext {
        return RequestContext {
            stream: stream,
            buf: Vec::new(),
            content_length: 0
        }
    }

    fn read_cb(&mut self, key:u64, epoll_fd: RawFd) -> io::Result<()> {
        let mut buf= [0u8; 4096];
        match self.stream.read(&mut buf) {
            Ok(_) => {
                if let Ok(data) = std::str::from_utf8(&buf) {
                    self.parse_and_set_content_length(data);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                return Err(e);
            }
        };

        self.buf.extend_from_slice(&buf);
        if self.buf.len() >= self.content_length {
            println!("got all data: {} bytes", self.buf.len());
            modify_interest(epoll_fd, self.stream.as_raw_fd(), listener_write_event(key))?;
        } else {
            modify_interest(epoll_fd, self.stream.as_raw_fd(), listener_read_event(key))?;
        }
        Ok(())
    }

    fn write_cb(&mut self, key: u64, epoll_fd: RawFd) -> io::Result<()> {
        match self.stream.write(HTTP_RESP) {
            Ok(_) => println!("发送响应成功！key : {}", key),
            Err(err) => eprint!("发送响应失败：key {}, err is {}", key, err),
        };
        self.stream.shutdown(std::net::Shutdown::Both)?;
        let fd = self.stream.as_raw_fd();
        remove_interest(epoll_fd, fd)?;
        syscall!(close(fd));
        Ok(())
    }
    
    fn parse_and_set_content_length(&mut self, data: &str) {
        if data.contains("HTTP") {
            if let Some(content_length) = data
                .lines()
                .find(|l| l.to_lowercase().starts_with("content-length: "))
            {
                if let Some(len) = content_length
                    .to_lowercase()
                    .strip_prefix("content-length: ")
                {
                    self.content_length = len.parse::<usize>().expect("content-length is valid");
                    println!("set content length: {} bytes", self.content_length);
                }
            }
        }
    }
}



fn main() -> io::Result<()> {
    let mut request_contexts: HashMap<u64, RequestContext> = HashMap::new();
    let listener = TcpListener::bind("127.0.0.1:8000")?;
    listener.set_nonblocking(true)?;
    // linux 一切皆文件。获取tcp fd。
    let listener_fd = listener.as_raw_fd();

    // 创建epoll队列
    let epoll_fd = epoll_create().expect("创建epoll 队列成功");
    
    // 
    let key = 888;
    // 在epoll 队列中 注册我们的自定义事件，下面的epoll_wait 接受到后会accept请求
    add_interst(epoll_fd, listener_fd, listener_read_event(key))?;

    let mut events: Vec<libc::epoll_event> = Vec::with_capacity(1024);
    let mut key = 888;

    loop {
        println!("等待处理的请求数: {}", request_contexts.len());
        events.clear();
        // 设置系统调用epoll_wait, 传入epoll_fd epoll 队列，接受队列，最大接受事件数，超时时间(毫秒)
        // 下面的系统调用要么1秒后超时返回，要么有事件返回，要么被信号处理中断
        // 如果timeout 设置为 -1， epoll_wait会等待直到其他两个事件发生
        let res = match syscall!(epoll_wait(
            epoll_fd, 
            events.as_mut_ptr() as *mut libc::epoll_event, 
            1024,
            1000 as libc::c_int
        )) {
            Ok(v) => v,
            Err(e) => panic!("epoll wait 失败：{}", e),
        };

        // Safe 设置的长度来自epoll_wait 设置的max_event=1024 < 设置的Vec capacity
        unsafe {
            events.set_len(res as usize);
        }

        
        for ev in &events {
            match ev.u64 {
                // 888 是上面放入的事件
                888 => {
                    match listener.accept() {
                        Ok((stream,addr))=> {
                            stream.set_nonblocking(true)?;
                            println!("新客户端： {}", addr);
                            key += 1;
                            add_interst(epoll_fd, stream.as_raw_fd(), listener_read_event(key))?;
                            request_contexts.insert(key, RequestContext::new(stream));
                        }
                        Err(e) => eprint!("服务端 accept 连接失败：{}", e),
                    };
                    modify_interest(epoll_fd, listener_fd, listener_read_event(888))?;
                }
                key => {
                    let mut to_delete = None;
                    if let Some(context) = request_contexts.get_mut(&key) {
                        let events: u32 = ev.events;
                        match events {
                            v if v as i32 & libc::EPOLLIN == libc::EPOLLIN => {
                                context.read_cb(key, epoll_fd)?;
                            }
                            v if v as i32 & libc::EPOLLOUT == libc::EPOLLOUT => {
                                context.write_cb(key, epoll_fd)?;
                                to_delete = Some(key);
                            }
                            v=> println!("未知事件：{}", v),
                        };
                    }
                    if let Some(key) = to_delete {
                        println!("remove key: {}", key);
                        request_contexts.remove(&key);
                    }
                }
            }
        }
    }
}
