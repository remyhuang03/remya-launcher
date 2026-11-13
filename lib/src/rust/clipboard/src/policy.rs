use crate::history::Preferences;
use crate::service::{Error, Service};
use futures_util::StreamExt;
use serde_json::Value;
use std::time::Duration;

const IDS: [&str; 3] = [
    "remya-launcher.clipboard-history",
    "remya-launcher.clipboard-max-items",
    "remya-launcher.clipboard-retention-days",
];

fn parse(values: &[Value; 3]) -> Result<Preferences, Error> {
    for (id, value) in IDS.iter().zip(values) {
        if value["id"] != *id || value["availability"] != "available" {
            return Err("Clipboard policy is unavailable".into());
        }
    }
    let integer = |index: usize, max| {
        values[index]["value"]
            .as_u64()
            .filter(|value| (1..=max).contains(value))
            .map(|value| value as u32)
            .ok_or_else(|| Error::from("Invalid clipboard policy integer"))
    };
    Ok(Preferences {
        enabled: values[0]["value"]
            .as_bool()
            .ok_or("Invalid clipboard history policy")?,
        max_items: integer(1, 5000)?,
        retention_days: integer(2, 365)?,
    })
}

async fn read(proxy: &zbus::Proxy<'_>) -> Result<Preferences, Error> {
    let mut values = [Value::Null, Value::Null, Value::Null];
    for (id, value) in IDS.iter().zip(&mut values) {
        let reply: String = proxy.call("Get", &(id,)).await?;
        *value = serde_json::from_str(&reply)?;
    }
    parse(&values)
}

async fn watch(service: &Service, retry: &mut u64) -> Result<(), Error> {
    let connection = zbus::connection::Builder::session()?
        .method_timeout(Duration::from_secs(5))
        .build()
        .await?;
    watch_connection(service, retry, &connection).await
}

async fn watch_connection(
    service: &Service,
    retry: &mut u64,
    connection: &zbus::Connection,
) -> Result<(), Error> {
    let proxy = zbus::Proxy::new(
        connection,
        "top.remya.Core1",
        "/top/remya/Core1",
        "top.remya.Core1",
    )
    .await?;
    // Subscribe first; queued invalidations cause a fresh read after the snapshot.
    let mut changes = proxy.receive_signal("Changed").await?;
    let mut owners = proxy.receive_owner_changed().await?;
    loop {
        service.apply_policy(Some(read(&proxy).await?))?;
        *retry = 1;
        loop {
            tokio::select! {
                signal = changes.next() => {
                    let signal = signal.ok_or("Core signal stream closed")?;
                    let (id, _): (String, String) = signal.body().deserialize()?;
                    if IDS.contains(&id.as_str()) { break; }
                }
                _ = owners.next() => {
                    return Err("Core service owner changed".into());
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => break,
            }
        }
    }
}

pub fn run(service: &Service) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Clipboard policy runtime unavailable: {error}");
            return;
        }
    };
    runtime.block_on(async {
        let mut retry = 1;
        loop {
            if let Err(error) = watch(service, &mut retry).await {
                let _ = service.apply_policy(None);
                eprintln!("Clipboard policy unavailable; capture and automatic cleanup paused: {error}; retry in {retry}s");
            }
            tokio::time::sleep(Duration::from_secs(retry)).await;
            retry = (retry * 2).min(30);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_entire_policy_before_applying() {
        let mut values = std::array::from_fn(|i| {
            json!({
                "id": IDS[i], "availability": "available", "value": ([json!(true), json!(500), json!(30)][i])
            })
        });
        assert_eq!(parse(&values).unwrap().max_items, 500);
        for invalid in [json!(0), json!(5001), json!(1.5), json!("500")] {
            values[1]["value"] = invalid;
            assert!(parse(&values).is_err());
        }
        values[1]["value"] = json!(500);
        values[0]["value"] = json!(1);
        assert!(parse(&values).is_err());
        values[0]["value"] = json!(true);
        values[2]["value"] = json!(366);
        assert!(parse(&values).is_err());
    }

    struct MockCore(std::sync::Mutex<[Value; 3]>);

    #[zbus::interface(name = "top.remya.Core1")]
    impl MockCore {
        fn get(&self, id: &str) -> String {
            let index = IDS.iter().position(|key| *key == id).unwrap();
            json!({"id": id, "availability": "available", "value": self.0.lock().unwrap()[index]})
                .to_string()
        }
    }

    #[tokio::test]
    async fn isolated_bus_get_changed_invalid_value_and_owner_loss() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        struct Bus(std::process::Child);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut bus = Bus(Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap());
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let server = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name("top.remya.Core1")
            .unwrap()
            .serve_at(
                "/top/remya/Core1",
                MockCore(std::sync::Mutex::new([json!(true), json!(500), json!(30)])),
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let service = std::sync::Arc::new(Service::new(
            crate::history::History::open(std::path::Path::new(":memory:")).unwrap(),
        ));
        let watching = service.clone();
        let task = tokio::spawn(async move {
            let mut retry = 1;
            let result = watch_connection(&watching, &mut retry, &connection).await;
            watching.apply_policy(None).unwrap();
            result
        });
        async fn wait(service: &Service, enabled: bool) {
            tokio::time::timeout(Duration::from_secs(3), async {
                while service.policy_enabled() != enabled {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
        wait(&service, true).await;
        let interface = server
            .object_server()
            .interface::<_, MockCore>("/top/remya/Core1")
            .await
            .unwrap();
        interface.get().await.0.lock().unwrap()[0] = json!(false);
        server
            .emit_signal(
                None::<&str>,
                "/top/remya/Core1",
                "top.remya.Core1",
                "Changed",
                &(IDS[0], "{}"),
            )
            .await
            .unwrap();
        wait(&service, false).await;
        interface.get().await.0.lock().unwrap()[0] = json!(true);
        server
            .emit_signal(
                None::<&str>,
                "/top/remya/Core1",
                "top.remya.Core1",
                "Changed",
                &(IDS[0], "{}"),
            )
            .await
            .unwrap();
        wait(&service, true).await;
        server.release_name("top.remya.Core1").await.unwrap();
        wait(&service, false).await;
        assert!(task.await.unwrap().is_err());

        let proxy = zbus::Proxy::new(
            &server,
            server.unique_name().unwrap(),
            "/top/remya/Core1",
            "top.remya.Core1",
        )
        .await
        .unwrap();
        interface.get().await.0.lock().unwrap()[2] = json!(0);
        assert!(read(&proxy).await.is_err());
    }
}
