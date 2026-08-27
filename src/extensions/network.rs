use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    rc::Rc,
    time::Duration,
};

use crate::native::{
    NativeCall, NativeFunction as Function, NativeRegistry, NativeSignature as Signature,
    NativeSpace as Space,
};
use crate::{Data, Result, Ty, Value, val};

const MAX_DATAGRAM: i64 = 65_535;
const MAX_HTTP_BYTES: i64 = 64 * 1024 * 1024;

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Bytes",
        functions: &[
            Function { name: "encode", call: bytes_encode },
            Function { name: "decode", call: bytes_decode },
        ],
        signatures: || {
            vec![
                Signature::exact("encode", vec![Ty::String], bytes_ty()),
                Signature::exact(
                    "decode",
                    vec![bytes_ty()],
                    Ty::Perchance(Box::new(Ty::String)),
                ),
            ]
        },
    });
    registry.add(Space {
        name: "Udp",
        functions: &[
            Function { name: "bind", call: udp_bind },
            Function { name: "connect", call: udp_connect },
            Function { name: "send", call: udp_send },
            Function { name: "send_bytes", call: udp_send_bytes },
            Function { name: "send_to", call: udp_send_to },
            Function { name: "send_bytes_to", call: udp_send_bytes_to },
            Function { name: "receive", call: udp_receive },
            Function { name: "ready", call: udp_ready },
            Function { name: "local_host", call: udp_local_host },
            Function { name: "local_port", call: udp_local_port },
            Function { name: "set_broadcast", call: udp_set_broadcast },
            Function { name: "set_nonblocking", call: udp_set_nonblocking },
        ],
        signatures: || {
            let socket = Ty::UdpSocket;
            vec![
                Signature::exact("bind", vec![Ty::String, Ty::Int], socket.clone()),
                Signature::exact(
                    "connect",
                    vec![socket.clone(), Ty::String, Ty::Int],
                    Ty::Unit,
                ),
                Signature::exact("send", vec![socket.clone(), Ty::String], Ty::Int),
                Signature::exact("send_bytes", vec![socket.clone(), bytes_ty()], Ty::Int),
                Signature::exact(
                    "send_to",
                    vec![socket.clone(), Ty::String, Ty::Int, Ty::String],
                    Ty::Int,
                ),
                Signature::exact(
                    "send_bytes_to",
                    vec![socket.clone(), Ty::String, Ty::Int, bytes_ty()],
                    Ty::Int,
                ),
                Signature::exact("receive", vec![socket.clone(), Ty::Int], Ty::UdpPacket),
                Signature::exact("ready", vec![socket.clone(), Ty::Int], Ty::Bool),
                Signature::exact("local_host", vec![socket.clone()], Ty::String),
                Signature::exact("local_port", vec![socket.clone()], Ty::Int),
                Signature::exact("set_broadcast", vec![socket.clone(), Ty::Bool], Ty::Unit),
                Signature::exact("set_nonblocking", vec![socket, Ty::Bool], Ty::Unit),
            ]
        },
    });
    registry.add(Space {
        name: "Tcp",
        functions: &[
            Function { name: "listen", call: tcp_listen },
            Function { name: "connect", call: tcp_connect },
            Function { name: "accept", call: tcp_accept },
            Function { name: "try_accept", call: tcp_try_accept },
            Function { name: "read", call: tcp_read },
            Function { name: "read_text", call: tcp_read_text },
            Function { name: "ready", call: tcp_ready },
            Function { name: "write", call: tcp_write },
            Function { name: "write_bytes", call: tcp_write_bytes },
            Function { name: "write_all", call: tcp_write_all },
            Function { name: "write_all_bytes", call: tcp_write_all_bytes },
            Function { name: "shutdown", call: tcp_shutdown },
            Function { name: "set_nonblocking", call: tcp_set_nonblocking },
            Function { name: "set_listener_nonblocking", call: tcp_set_listener_nonblocking },
            Function { name: "set_nodelay", call: tcp_set_nodelay },
            Function { name: "local_host", call: tcp_local_host },
            Function { name: "local_port", call: tcp_local_port },
            Function { name: "peer_host", call: tcp_peer_host },
            Function { name: "peer_port", call: tcp_peer_port },
            Function { name: "listener_host", call: tcp_listener_host },
            Function { name: "listener_port", call: tcp_listener_port },
        ],
        signatures: || {
            let listener = Ty::TcpListener;
            let stream = Ty::TcpStream;
            vec![
                Signature::exact("listen", vec![Ty::String, Ty::Int], listener.clone()),
                Signature::exact(
                    "connect",
                    vec![Ty::String, Ty::Int, Ty::Int],
                    stream.clone(),
                ),
                Signature::exact("accept", vec![listener.clone()], stream.clone()),
                Signature::exact(
                    "try_accept",
                    vec![listener.clone()],
                    Ty::Perchance(Box::new(stream.clone())),
                ),
                Signature::exact("read", vec![stream.clone(), Ty::Int], bytes_ty()),
                Signature::exact(
                    "read_text",
                    vec![stream.clone(), Ty::Int],
                    Ty::Perchance(Box::new(Ty::String)),
                ),
                Signature::exact("ready", vec![stream.clone(), Ty::Int], Ty::Bool),
                Signature::exact("write", vec![stream.clone(), Ty::String], Ty::Int),
                Signature::exact("write_bytes", vec![stream.clone(), bytes_ty()], Ty::Int),
                Signature::exact("write_all", vec![stream.clone(), Ty::String], Ty::Unit),
                Signature::exact(
                    "write_all_bytes",
                    vec![stream.clone(), bytes_ty()],
                    Ty::Unit,
                ),
                Signature::exact("shutdown", vec![stream.clone(), Ty::String], Ty::Unit),
                Signature::exact(
                    "set_nonblocking",
                    vec![stream.clone(), Ty::Bool],
                    Ty::Unit,
                ),
                Signature::exact(
                    "set_listener_nonblocking",
                    vec![listener.clone(), Ty::Bool],
                    Ty::Unit,
                ),
                Signature::exact("set_nodelay", vec![stream.clone(), Ty::Bool], Ty::Unit),
                Signature::exact("local_host", vec![stream.clone()], Ty::String),
                Signature::exact("local_port", vec![stream.clone()], Ty::Int),
                Signature::exact("peer_host", vec![stream.clone()], Ty::String),
                Signature::exact("peer_port", vec![stream], Ty::Int),
                Signature::exact("listener_host", vec![listener.clone()], Ty::String),
                Signature::exact("listener_port", vec![listener], Ty::Int),
            ]
        },
    });
    registry.add(Space {
        name: "Http",
        functions: &[
            Function { name: "get", call: http_get },
            Function { name: "request", call: http_request },
        ],
        signatures: || {
            let headers = Ty::Map(Box::new(Ty::String), Box::new(Ty::String));
            vec![
                Signature::exact(
                    "get",
                    vec![
                        Ty::String,
                        Ty::Int,
                        Ty::String,
                        headers.clone(),
                        Ty::Int,
                        Ty::Int,
                    ],
                    Ty::HttpResponse,
                ),
                Signature::exact(
                    "request",
                    vec![
                        Ty::String,
                        Ty::String,
                        Ty::Int,
                        Ty::String,
                        headers,
                        bytes_ty(),
                        Ty::Int,
                        Ty::Int,
                    ],
                    Ty::HttpResponse,
                ),
            ]
        },
    });
}

fn bytes_ty() -> Ty {
    Ty::Arr(Box::new(Ty::Int))
}

fn byte_array(bytes: &[u8]) -> Value {
    val(
        bytes_ty(),
        Data::Arr(Rc::new(RefCell::new(
            bytes
                .iter()
                .map(|byte| val(Ty::Int, Data::Int(i64::from(*byte))))
                .collect(),
        ))),
    )
}

fn bytes(value: &Value, call: &NativeCall<'_>, operation: &str) -> Result<Vec<u8>> {
    let (Ty::Arr(element), Data::Arr(items)) = (&value.ty, &value.data) else {
        return Err(call.error(format!("{operation} expects arr[int] bytes")));
    };
    if **element != Ty::Int {
        return Err(call.error(format!("{operation} expects arr[int] bytes")));
    }
    items
        .borrow()
        .iter()
        .map(|item| match item.data {
            Data::Int(byte) if (0..=255).contains(&byte) => Ok(byte as u8),
            _ => Err(call.error(format!("{operation} byte values must be between 0 and 255"))),
        })
        .collect()
}

fn optional_text(bytes: &[u8]) -> Value {
    let ty = Ty::Perchance(Box::new(Ty::String));
    match std::str::from_utf8(bytes) {
        Ok(text) => val(ty, Data::String(text.to_owned())),
        Err(_) => val(ty, Data::Naught),
    }
}

fn bytes_encode(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Bytes.encode")?;
    Ok(byte_array(call.string(0, "Bytes.encode")?.as_bytes()))
}

fn bytes_decode(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Bytes.decode")?;
    let encoded = bytes(call.value(0, "Bytes.decode")?, &call, "Bytes.decode")?;
    Ok(optional_text(&encoded))
}

fn port(value: i64, call: &NativeCall<'_>, operation: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| call.error(format!("{operation} port must be 0 through 65535")))
}

fn timeout(value: i64, call: &NativeCall<'_>, operation: &str) -> Result<Duration> {
    if value < 0 {
        return Err(call.error(format!("{operation} timeout cannot be negative")));
    }
    Ok(Duration::from_millis(value as u64))
}

fn limit(value: i64, maximum: i64, call: &NativeCall<'_>, operation: &str) -> Result<usize> {
    if !(1..=maximum).contains(&value) {
        return Err(call.error(format!("{operation} byte limit must be 1 through {maximum}")));
    }
    Ok(value as usize)
}

fn addresses(host: &str, port: u16, call: &NativeCall<'_>, operation: &str) -> Result<Vec<SocketAddr>> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| call.error(format!("{operation} could not resolve {host}: {error}")))?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(call.error(format!("{operation} resolved no address for {host}")));
    }
    Ok(addresses)
}

fn udp(call: &NativeCall<'_>, operation: &str) -> Result<Rc<UdpSocket>> {
    match &call.value(0, operation)?.data {
        Data::UdpSocket(socket) => Ok(socket.clone()),
        _ => Err(call.error(format!("{operation} expects udp_socket"))),
    }
}

fn udp_bind(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.bind")?;
    let host = call.string(0, "Udp.bind")?;
    let port = port(call.int(1, "Udp.bind")?, &call, "Udp.bind")?;
    let socket = UdpSocket::bind((host, port))
        .map_err(|error| call.error(format!("Udp.bind failed: {error}")))?;
    Ok(val(Ty::UdpSocket, Data::UdpSocket(Rc::new(socket))))
}

fn udp_connect(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "Udp.connect")?;
    let socket = udp(&call, "Udp.connect")?;
    let host = call.string(1, "Udp.connect")?;
    let port = port(call.int(2, "Udp.connect")?, &call, "Udp.connect")?;
    socket
        .connect((host, port))
        .map_err(|error| call.error(format!("Udp.connect failed: {error}")))?;
    Ok(call.unit_value())
}

fn udp_send(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.send")?;
    let socket = udp(&call, "Udp.send")?;
    let sent = socket
        .send(call.string(1, "Udp.send")?.as_bytes())
        .map_err(|error| call.error(format!("Udp.send failed: {error}")))?;
    Ok(call.int_value(sent as i64))
}

fn udp_send_bytes(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.send_bytes")?;
    let socket = udp(&call, "Udp.send_bytes")?;
    let payload = bytes(call.value(1, "Udp.send_bytes")?, &call, "Udp.send_bytes")?;
    let sent = socket
        .send(&payload)
        .map_err(|error| call.error(format!("Udp.send_bytes failed: {error}")))?;
    Ok(call.int_value(sent as i64))
}

fn udp_send_to(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(4, "Udp.send_to")?;
    let payload = call.string(3, "Udp.send_to")?.as_bytes();
    udp_send_to_inner(&call, "Udp.send_to", payload)
}

fn udp_send_bytes_to(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(4, "Udp.send_bytes_to")?;
    let payload = bytes(
        call.value(3, "Udp.send_bytes_to")?,
        &call,
        "Udp.send_bytes_to",
    )?;
    udp_send_to_inner(&call, "Udp.send_bytes_to", &payload)
}

fn udp_send_to_inner(call: &NativeCall<'_>, operation: &str, payload: &[u8]) -> Result<Value> {
    let socket = udp(call, operation)?;
    let host = call.string(1, operation)?;
    let port = port(call.int(2, operation)?, call, operation)?;
    let sent = socket
        .send_to(payload, (host, port))
        .map_err(|error| call.error(format!("{operation} failed: {error}")))?;
    Ok(call.int_value(sent as i64))
}

fn udp_receive(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.receive")?;
    let socket = udp(&call, "Udp.receive")?;
    let maximum = limit(
        call.int(1, "Udp.receive")?,
        MAX_DATAGRAM,
        &call,
        "Udp.receive",
    )?;
    let mut buffer = vec![0; maximum];
    let (size, source) = socket
        .recv_from(&mut buffer)
        .map_err(|error| call.error(format!("Udp.receive failed: {error}")))?;
    buffer.truncate(size);
    Ok(val(
        Ty::UdpPacket,
        Data::UdpPacket {
            host: source.ip().to_string(),
            port: i64::from(source.port()),
            bytes: buffer,
        },
    ))
}

fn udp_ready(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.ready")?;
    let socket = udp(&call, "Udp.ready")?;
    let duration = timeout(call.int(1, "Udp.ready")?, &call, "Udp.ready")?;
    let original = socket
        .read_timeout()
        .map_err(|error| call.error(format!("Udp.ready failed: {error}")))?;
    socket
        .set_read_timeout(Some(duration.max(Duration::from_millis(1))))
        .map_err(|error| call.error(format!("Udp.ready failed: {error}")))?;
    let ready = match socket.peek_from(&mut [0]) {
        Ok(_) => true,
        Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => false,
        Err(error) => return Err(call.error(format!("Udp.ready failed: {error}"))),
    };
    socket
        .set_read_timeout(original)
        .map_err(|error| call.error(format!("Udp.ready failed: {error}")))?;
    Ok(call.bool_value(ready))
}

fn udp_local_host(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Udp.local_host")?;
    let address = udp(&call, "Udp.local_host")?
        .local_addr()
        .map_err(|error| call.error(format!("Udp.local_host failed: {error}")))?;
    Ok(call.string_value(address.ip().to_string()))
}

fn udp_local_port(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Udp.local_port")?;
    let address = udp(&call, "Udp.local_port")?
        .local_addr()
        .map_err(|error| call.error(format!("Udp.local_port failed: {error}")))?;
    Ok(call.int_value(i64::from(address.port())))
}

fn udp_set_broadcast(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.set_broadcast")?;
    udp(&call, "Udp.set_broadcast")?
        .set_broadcast(call.bool(1, "Udp.set_broadcast")?)
        .map_err(|error| call.error(format!("Udp.set_broadcast failed: {error}")))?;
    Ok(call.unit_value())
}

fn udp_set_nonblocking(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Udp.set_nonblocking")?;
    udp(&call, "Udp.set_nonblocking")?
        .set_nonblocking(call.bool(1, "Udp.set_nonblocking")?)
        .map_err(|error| call.error(format!("Udp.set_nonblocking failed: {error}")))?;
    Ok(call.unit_value())
}

fn listener(call: &NativeCall<'_>, operation: &str) -> Result<Rc<TcpListener>> {
    match &call.value(0, operation)?.data {
        Data::TcpListener(listener) => Ok(listener.clone()),
        _ => Err(call.error(format!("{operation} expects tcp_listener"))),
    }
}

fn stream(call: &NativeCall<'_>, operation: &str) -> Result<Rc<RefCell<TcpStream>>> {
    match &call.value(0, operation)?.data {
        Data::TcpStream(stream) => Ok(stream.clone()),
        _ => Err(call.error(format!("{operation} expects tcp_stream"))),
    }
}

fn stream_value(stream: TcpStream) -> Value {
    val(Ty::TcpStream, Data::TcpStream(Rc::new(RefCell::new(stream))))
}

fn tcp_listen(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.listen")?;
    let host = call.string(0, "Tcp.listen")?;
    let port = port(call.int(1, "Tcp.listen")?, &call, "Tcp.listen")?;
    let listener = TcpListener::bind((host, port))
        .map_err(|error| call.error(format!("Tcp.listen failed: {error}")))?;
    Ok(val(Ty::TcpListener, Data::TcpListener(Rc::new(listener))))
}

fn tcp_connect(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "Tcp.connect")?;
    let host = call.string(0, "Tcp.connect")?;
    let port = port(call.int(1, "Tcp.connect")?, &call, "Tcp.connect")?;
    let timeout = timeout(call.int(2, "Tcp.connect")?, &call, "Tcp.connect")?;
    let mut last_error = None;
    for address in addresses(host, port, &call, "Tcp.connect")? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream_value(stream)),
            Err(error) => last_error = Some(error),
        }
    }
    Err(call.error(format!(
        "Tcp.connect failed: {}",
        last_error.expect("resolved addresses are not empty")
    )))
}

fn tcp_accept(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.accept")?;
    let (stream, _) = listener(&call, "Tcp.accept")?
        .accept()
        .map_err(|error| call.error(format!("Tcp.accept failed: {error}")))?;
    Ok(stream_value(stream))
}

fn tcp_try_accept(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.try_accept")?;
    let ty = Ty::Perchance(Box::new(Ty::TcpStream));
    match listener(&call, "Tcp.try_accept")?.accept() {
        Ok((stream, _)) => {
            let mut value = stream_value(stream);
            value.ty = ty;
            Ok(value)
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(val(ty, Data::Naught)),
        Err(error) => Err(call.error(format!("Tcp.try_accept failed: {error}"))),
    }
}

fn tcp_read(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.read")?;
    let maximum = limit(call.int(1, "Tcp.read")?, i64::from(u16::MAX), &call, "Tcp.read")?;
    let stream = stream(&call, "Tcp.read")?;
    let mut buffer = vec![0; maximum];
    let size = stream
        .borrow_mut()
        .read(&mut buffer)
        .map_err(|error| call.error(format!("Tcp.read failed: {error}")))?;
    buffer.truncate(size);
    Ok(byte_array(&buffer))
}

fn tcp_read_text(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.read_text")?;
    let maximum = limit(
        call.int(1, "Tcp.read_text")?,
        i64::from(u16::MAX),
        &call,
        "Tcp.read_text",
    )?;
    let stream = stream(&call, "Tcp.read_text")?;
    let mut buffer = vec![0; maximum];
    let size = stream
        .borrow_mut()
        .read(&mut buffer)
        .map_err(|error| call.error(format!("Tcp.read_text failed: {error}")))?;
    buffer.truncate(size);
    Ok(optional_text(&buffer))
}

fn tcp_ready(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.ready")?;
    let duration = timeout(call.int(1, "Tcp.ready")?, &call, "Tcp.ready")?;
    let stream = stream(&call, "Tcp.ready")?;
    let stream = stream.borrow();
    let original = stream
        .read_timeout()
        .map_err(|error| call.error(format!("Tcp.ready failed: {error}")))?;
    stream
        .set_read_timeout(Some(duration.max(Duration::from_millis(1))))
        .map_err(|error| call.error(format!("Tcp.ready failed: {error}")))?;
    let ready = match stream.peek(&mut [0]) {
        Ok(_) => true,
        Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => false,
        Err(error) => return Err(call.error(format!("Tcp.ready failed: {error}"))),
    };
    stream
        .set_read_timeout(original)
        .map_err(|error| call.error(format!("Tcp.ready failed: {error}")))?;
    Ok(call.bool_value(ready))
}

fn tcp_write(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.write")?;
    let stream = stream(&call, "Tcp.write")?;
    let size = stream
        .borrow_mut()
        .write(call.string(1, "Tcp.write")?.as_bytes())
        .map_err(|error| call.error(format!("Tcp.write failed: {error}")))?;
    Ok(call.int_value(size as i64))
}

fn tcp_write_bytes(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.write_bytes")?;
    let stream = stream(&call, "Tcp.write_bytes")?;
    let payload = bytes(call.value(1, "Tcp.write_bytes")?, &call, "Tcp.write_bytes")?;
    let size = stream
        .borrow_mut()
        .write(&payload)
        .map_err(|error| call.error(format!("Tcp.write_bytes failed: {error}")))?;
    Ok(call.int_value(size as i64))
}

fn tcp_write_all(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.write_all")?;
    let stream = stream(&call, "Tcp.write_all")?;
    stream
        .borrow_mut()
        .write_all(call.string(1, "Tcp.write_all")?.as_bytes())
        .map_err(|error| call.error(format!("Tcp.write_all failed: {error}")))?;
    Ok(call.unit_value())
}

fn tcp_write_all_bytes(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.write_all_bytes")?;
    let stream = stream(&call, "Tcp.write_all_bytes")?;
    let payload = bytes(
        call.value(1, "Tcp.write_all_bytes")?,
        &call,
        "Tcp.write_all_bytes",
    )?;
    stream
        .borrow_mut()
        .write_all(&payload)
        .map_err(|error| call.error(format!("Tcp.write_all_bytes failed: {error}")))?;
    Ok(call.unit_value())
}

fn tcp_shutdown(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.shutdown")?;
    let how = match call.string(1, "Tcp.shutdown")? {
        "read" => Shutdown::Read,
        "write" => Shutdown::Write,
        "both" => Shutdown::Both,
        _ => return Err(call.error("Tcp.shutdown mode must be read, write, or both")),
    };
    stream(&call, "Tcp.shutdown")?
        .borrow()
        .shutdown(how)
        .map_err(|error| call.error(format!("Tcp.shutdown failed: {error}")))?;
    Ok(call.unit_value())
}

fn tcp_set_nonblocking(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.set_nonblocking")?;
    stream(&call, "Tcp.set_nonblocking")?
        .borrow()
        .set_nonblocking(call.bool(1, "Tcp.set_nonblocking")?)
        .map_err(|error| call.error(format!("Tcp.set_nonblocking failed: {error}")))?;
    Ok(call.unit_value())
}

fn tcp_set_listener_nonblocking(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.set_listener_nonblocking")?;
    listener(&call, "Tcp.set_listener_nonblocking")?
        .set_nonblocking(call.bool(1, "Tcp.set_listener_nonblocking")?)
        .map_err(|error| call.error(format!("Tcp.set_listener_nonblocking failed: {error}")))?;
    Ok(call.unit_value())
}

fn tcp_set_nodelay(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Tcp.set_nodelay")?;
    stream(&call, "Tcp.set_nodelay")?
        .borrow()
        .set_nodelay(call.bool(1, "Tcp.set_nodelay")?)
        .map_err(|error| call.error(format!("Tcp.set_nodelay failed: {error}")))?;
    Ok(call.unit_value())
}

fn socket_host(call: &NativeCall<'_>, operation: &str, address: io::Result<SocketAddr>) -> Result<Value> {
    let address = address.map_err(|error| call.error(format!("{operation} failed: {error}")))?;
    Ok(call.string_value(address.ip().to_string()))
}

fn socket_port(call: &NativeCall<'_>, operation: &str, address: io::Result<SocketAddr>) -> Result<Value> {
    let address = address.map_err(|error| call.error(format!("{operation} failed: {error}")))?;
    Ok(call.int_value(i64::from(address.port())))
}

fn tcp_local_host(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.local_host")?;
    socket_host(&call, "Tcp.local_host", stream(&call, "Tcp.local_host")?.borrow().local_addr())
}
fn tcp_local_port(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.local_port")?;
    socket_port(&call, "Tcp.local_port", stream(&call, "Tcp.local_port")?.borrow().local_addr())
}
fn tcp_peer_host(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.peer_host")?;
    socket_host(&call, "Tcp.peer_host", stream(&call, "Tcp.peer_host")?.borrow().peer_addr())
}
fn tcp_peer_port(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.peer_port")?;
    socket_port(&call, "Tcp.peer_port", stream(&call, "Tcp.peer_port")?.borrow().peer_addr())
}
fn tcp_listener_host(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.listener_host")?;
    socket_host(&call, "Tcp.listener_host", listener(&call, "Tcp.listener_host")?.local_addr())
}
fn tcp_listener_port(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Tcp.listener_port")?;
    socket_port(&call, "Tcp.listener_port", listener(&call, "Tcp.listener_port")?.local_addr())
}

fn header_map(value: &Value, call: &NativeCall<'_>, operation: &str) -> Result<BTreeMap<String, String>> {
    let Data::Map(entries) = &value.data else {
        return Err(call.error(format!("{operation} headers must be map[string, string]")));
    };
    entries
        .borrow()
        .iter()
        .map(|(key, value)| {
            let name = key
                .strip_prefix("t:")
                .ok_or_else(|| call.error(format!("{operation} header names must be string")))?;
            let Data::String(value) = &value.data else {
                return Err(call.error(format!("{operation} header values must be string")));
            };
            if name.is_empty() || name.contains(['\r', '\n', ':']) || value.contains(['\r', '\n']) {
                return Err(call.error(format!("{operation} contains an invalid HTTP header")));
            }
            Ok((name.to_owned(), value.clone()))
        })
        .collect()
}

fn http_get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(6, "Http.get")?;
    let headers = header_map(call.value(3, "Http.get")?, &call, "Http.get")?;
    http_exchange(
        &call,
        "GET",
        call.string(0, "Http.get")?,
        call.int(1, "Http.get")?,
        call.string(2, "Http.get")?,
        headers,
        &[],
        call.int(4, "Http.get")?,
        call.int(5, "Http.get")?,
    )
}

fn http_request(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(8, "Http.request")?;
    let headers = header_map(call.value(4, "Http.request")?, &call, "Http.request")?;
    let body = bytes(call.value(5, "Http.request")?, &call, "Http.request")?;
    http_exchange(
        &call,
        call.string(0, "Http.request")?,
        call.string(1, "Http.request")?,
        call.int(2, "Http.request")?,
        call.string(3, "Http.request")?,
        headers,
        &body,
        call.int(6, "Http.request")?,
        call.int(7, "Http.request")?,
    )
}

#[allow(clippy::too_many_arguments)]
fn http_exchange(
    call: &NativeCall<'_>,
    method: &str,
    host: &str,
    port_value: i64,
    target: &str,
    mut headers: BTreeMap<String, String>,
    body: &[u8],
    timeout_value: i64,
    maximum_value: i64,
) -> Result<Value> {
    if method.is_empty()
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'-')
    {
        return Err(call.error("Http.request method must use uppercase ASCII letters or '-'"));
    }
    if !target.starts_with('/') || target.contains(['\r', '\n', ' ']) {
        return Err(call.error("HTTP target must begin with '/' and contain no spaces or newlines"));
    }
    let port = port(port_value, call, "HTTP")?;
    let duration = timeout(timeout_value, call, "HTTP")?;
    let maximum = limit(maximum_value, MAX_HTTP_BYTES, call, "HTTP")?;
    let mut stream = None;
    let mut last_error = None;
    for address in addresses(host, port, call, "HTTP")? {
        match TcpStream::connect_timeout(&address, duration) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let mut stream = stream.ok_or_else(|| {
        call.error(format!(
            "HTTP connect failed: {}",
            last_error.expect("resolved addresses are not empty")
        ))
    })?;
    stream
        .set_read_timeout(Some(duration.max(Duration::from_millis(1))))
        .and_then(|_| stream.set_write_timeout(Some(duration.max(Duration::from_millis(1)))))
        .map_err(|error| call.error(format!("HTTP timeout setup failed: {error}")))?;

    let has_host = headers.keys().any(|name| name.eq_ignore_ascii_case("host"));
    let has_connection = headers
        .keys()
        .any(|name| name.eq_ignore_ascii_case("connection"));
    let has_length = headers
        .keys()
        .any(|name| name.eq_ignore_ascii_case("content-length"));
    if !has_host {
        let shown_host = if port == 80 {
            host.to_owned()
        } else {
            format!("{host}:{port}")
        };
        headers.insert("Host".into(), shown_host);
    }
    if !has_connection {
        headers.insert("Connection".into(), "close".into());
    }
    if !has_length {
        headers.insert("Content-Length".into(), body.len().to_string());
    }

    let mut request = format!("{method} {target} HTTP/1.1\r\n").into_bytes();
    for (name, value) in headers {
        request.extend_from_slice(name.as_bytes());
        request.extend_from_slice(b": ");
        request.extend_from_slice(value.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    request.extend_from_slice(b"\r\n");
    request.extend_from_slice(body);
    stream
        .write_all(&request)
        .map_err(|error| call.error(format!("HTTP write failed: {error}")))?;

    let mut response = Vec::new();
    Read::by_ref(&mut stream)
        .take((maximum + 1) as u64)
        .read_to_end(&mut response)
        .map_err(|error| call.error(format!("HTTP read failed: {error}")))?;
    if response.len() > maximum {
        return Err(call.error("HTTP response exceeded the configured byte limit"));
    }
    parse_response(response).map_err(|message| call.error(message))
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_response(response: Vec<u8>) -> std::result::Result<Value, String> {
    let end = header_end(&response).ok_or("HTTP response has no complete header block")?;
    let head = std::str::from_utf8(&response[..end])
        .map_err(|_| "HTTP response headers are not UTF-8")?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("HTTP response has no status line")?;
    let mut status_parts = status_line.splitn(3, ' ');
    let version = status_parts
        .next()
        .filter(|version| version.starts_with("HTTP/"))
        .ok_or("HTTP response has an invalid version")?
        .to_owned();
    let status = status_parts
        .next()
        .ok_or("HTTP response has no status code")?
        .parse::<i64>()
        .map_err(|_| "HTTP response has an invalid status code")?;
    let reason = status_parts.next().unwrap_or("").to_owned();
    let mut headers = BTreeMap::<String, String>::new();
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or("HTTP response contains a malformed header")?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err("HTTP response contains an empty header name".into());
        }
        let value = value.trim().to_owned();
        headers
            .entry(name)
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(&value);
            })
            .or_insert(value);
    }
    let mut body = response[end + 4..].to_vec();
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    {
        body = decode_chunked(&body)?;
    } else if let Some(length) = headers.get("content-length") {
        let length = length
            .parse::<usize>()
            .map_err(|_| "HTTP response has an invalid Content-Length")?;
        if body.len() < length {
            return Err("HTTP response body is shorter than Content-Length".into());
        }
        body.truncate(length);
    }
    Ok(val(
        Ty::HttpResponse,
        Data::HttpResponse {
            status,
            reason,
            version,
            headers,
            body,
        },
    ))
}

fn decode_chunked(encoded: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let mut remaining = encoded;
    let mut decoded = Vec::new();
    loop {
        let line_end = remaining
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("HTTP chunk has no size terminator")?;
        let size_text = std::str::from_utf8(&remaining[..line_end])
            .map_err(|_| "HTTP chunk size is not ASCII")?;
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "HTTP chunk has an invalid size")?;
        remaining = &remaining[line_end + 2..];
        if size == 0 {
            return Ok(decoded);
        }
        if remaining.len() < size + 2 || &remaining[size..size + 2] != b"\r\n" {
            return Err("HTTP chunk is truncated".into());
        }
        decoded.extend_from_slice(&remaining[..size]);
        remaining = &remaining[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    use super::{
        Data, NativeCall, Ty, decode_chunked, http_exchange, parse_response, tcp_accept, tcp_listen,
        tcp_read, udp_bind, udp_receive, udp_send_to,
    };
    use crate::{Value, val};

    fn string(value: &str) -> Value {
        val(Ty::String, Data::String(value.into()))
    }

    fn int(value: i64) -> Value {
        val(Ty::Int, Data::Int(value))
    }

    #[test]
    fn decodes_chunked_http_bodies() {
        assert_eq!(
            decode_chunked(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n").unwrap(),
            b"Wikipedia"
        );
    }

    #[test]
    fn parses_http_response_metadata_and_bytes() {
        let response = parse_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-Test: yes\r\n\r\nhello".to_vec(),
        )
        .unwrap();
        let Data::HttpResponse {
            status,
            headers,
            body,
            ..
        } = response.data
        else {
            panic!("expected HTTP response");
        };
        assert_eq!(status, 200);
        assert_eq!(headers.get("x-test").map(String::as_str), Some("yes"));
        assert_eq!(body, b"hello");
    }

    #[test]
    fn sends_binary_udp_datagrams_over_loopback() {
        let server_args = [string("127.0.0.1"), int(0)];
        let server = udp_bind(NativeCall::new(&server_args, 1)).unwrap();
        let Data::UdpSocket(server_socket) = &server.data else {
            panic!("expected UDP socket");
        };
        let port = i64::from(server_socket.local_addr().unwrap().port());

        let client_args = [string("127.0.0.1"), int(0)];
        let client = udp_bind(NativeCall::new(&client_args, 1)).unwrap();
        let send_args = [client, string("127.0.0.1"), int(port), string("hello")];
        udp_send_to(NativeCall::new(&send_args, 1)).unwrap();

        let receive_args = [server, int(64)];
        let packet = udp_receive(NativeCall::new(&receive_args, 1)).unwrap();
        let Data::UdpPacket { bytes, .. } = packet.data else {
            panic!("expected UDP packet");
        };
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn accepts_and_reads_tcp_streams_over_loopback() {
        let listen_args = [string("127.0.0.1"), int(0)];
        let listener = tcp_listen(NativeCall::new(&listen_args, 1)).unwrap();
        let Data::TcpListener(socket) = &listener.data else {
            panic!("expected TCP listener");
        };
        let port = socket.local_addr().unwrap().port();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"hello").unwrap();
        });

        let accept_args = [listener];
        let stream = tcp_accept(NativeCall::new(&accept_args, 1)).unwrap();
        let read_args = [stream, int(64)];
        let bytes = tcp_read(NativeCall::new(&read_args, 1)).unwrap();
        let Data::Arr(bytes) = bytes.data else {
            panic!("expected byte array");
        };
        let decoded = bytes
            .borrow()
            .iter()
            .map(|value| match value.data {
                Data::Int(byte) => byte as u8,
                _ => panic!("expected byte"),
            })
            .collect::<Vec<_>>();
        assert_eq!(decoded, b"hello");
        client.join().unwrap();
    }

    #[test]
    fn performs_plain_http_over_a_real_tcp_connection() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let size = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..size]).starts_with("GET /health HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
        });

        let no_arguments = [];
        let response = http_exchange(
            &NativeCall::new(&no_arguments, 1),
            "GET",
            "127.0.0.1",
            i64::from(port),
            "/health",
            BTreeMap::new(),
            &[],
            2_000,
            4_096,
        )
        .unwrap();
        let Data::HttpResponse { status, body, .. } = response.data else {
            panic!("expected HTTP response");
        };
        assert_eq!(status, 200);
        assert_eq!(body, b"ok");
        server.join().unwrap();
    }
}
