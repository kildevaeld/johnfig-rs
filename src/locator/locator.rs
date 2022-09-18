use std::path::PathBuf;

pub type BoxIterator<'a> = Box<dyn Iterator<Item = PathBuf> + 'a>;

pub trait Locator {
    // type Iter: Iterator<Item = PathBuf>;
    type Error;
    fn root(&self) -> &PathBuf;

    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> Result<BoxIterator<'a>, Self::Error>;
}

pub type BoxLocator = Box<dyn Locator<Error = Box<dyn std::error::Error>> + Send + Sync>;

struct LocatorBox<L>(L);

impl<L> Locator for LocatorBox<L>
where
    L: Locator,
    L::Error: 'static + std::error::Error,
{
    type Error = Box<dyn std::error::Error>;

    fn root(&self) -> &PathBuf {
        self.0.root()
    }

    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> Result<BoxIterator<'a>, Self::Error> {
        let iter = self.0.locate(search_names)?;
        Ok(iter)
    }
}

pub fn locatorbox<L: Locator + 'static>(locator: L) -> BoxLocator
where
    L::Error: std::error::Error + 'static,
    L: Send + Sync,
{
    Box::new(LocatorBox(locator))
}
