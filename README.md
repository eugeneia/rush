![Rush](rush.png)

# Rush: a simple, fast software networking toolkit written in Rust

Rush is a simple but fast toolkit for writing high-performance networking
applications in userspace using Rust.

Rush uses [kernel-bypass networking](https://github.com/lukego/blog/issues/13)
and [simple, efficient data structures](https://github.com/lukego/blog/issues/11)
to help you get the most out of your hardware.

Rush is [Snabb](https://github.com/snabbco/snabb/) written in Rust. It doesn’t
have all the bells and whistles of its big sibling Snabb, but it’s lean and
just as fast!

## Uses in the Wild

Rush was used to build [Synthetic Network](https://www.daily.co/blog/using-docker-and-userspace-networking-to-simulate-real-world-networks/),
a tool for simulating realistic network conditions to containerized
applications.


## Screencast

**[Screencast on porting Snabb to Rust](https://mr.gy/screen/rush/)**
