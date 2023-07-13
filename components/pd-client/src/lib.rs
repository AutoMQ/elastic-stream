#![feature(fn_traits)]

#[allow(clippy::type_complexity)]
pub struct PlacementDriverClient<'a> {
    listeners: Vec<Box<dyn FnMut(&str) + 'a>>,
}

impl<'a> PlacementDriverClient<'a> {
    pub fn new() -> Self {
        Self { listeners: vec![] }
    }

    pub fn add_listener<T>(&mut self, listener: T)
    where
        T: FnMut(&str) + 'a,
    {
        let listener = Box::new(listener);
        self.listeners.push(listener);
    }

    #[allow(dead_code)]
    fn apply(&mut self, change: &str) {
        self.listeners.iter_mut().for_each(|listener| {
            listener.call_mut((change,));
        })
    }
}

impl<'a> Default for PlacementDriverClient<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::debug;
    use std::error::Error;

    #[test]
    fn test_pd_client_listener() -> Result<(), Box<dyn Error>> {
        env_logger::try_init()?;

        let mut v = 0;

        let mut pd_client = PlacementDriverClient::new();
        let listener = |change: &str| {
            debug!("Hello {}", change);
            // Note v is captured by mutable reference
            v += 1;
        };
        pd_client.add_listener(listener);

        let listener = |change: &str| {
            debug!("Hi {}", change);
        };
        pd_client.add_listener(listener);

        pd_client.apply("Janet");

        drop(pd_client);
        assert_eq!(v, 1, "The first listener should have increment v by 1");
        Ok(())
    }
}
