//! Resolve every link before comparing physical directory identities.
use super::windows::identity::{self, Identity};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug)]
pub struct Target {
    pub path: PathBuf,
    pub identity: Identity,
    pub resolved: Identity,
}
impl Target {
    pub fn capture(path: PathBuf) -> anyhow::Result<Self> {
        anyhow::ensure!(std::fs::metadata(&path)?.is_dir(), "目标不是文件夹");
        let identity = identity::capture(&path)?.identity;
        let resolved = identity::capture(&path.canonicalize()?)?.identity;
        Ok(Self {
            path,
            identity,
            resolved,
        })
    }
    pub fn verify(&self) -> anyhow::Result<()> {
        let current = Self::capture(self.path.clone())?;
        anyhow::ensure!(
            self.identity == current.identity && self.resolved == current.resolved,
            "目标已改变，请重新拖放"
        );
        Ok(())
    }
}
pub fn ancestors(target: &Path) -> anyhow::Result<Vec<Identity>> {
    let path = target.canonicalize()?;
    let mut values = Vec::new();
    for p in path.ancestors() {
        anyhow::ensure!(values.len() < 1024, "目录层级过深，无法安全移动");
        values.push(identity::capture(p)?.identity);
    }
    anyhow::ensure!(!values.is_empty(), "无法核实目标目录");
    Ok(values)
}
pub fn check(source: &Path, ancestors: &[Identity]) -> anyhow::Result<()> {
    if std::fs::metadata(source)?.is_dir() {
        let resolved = identity::capture(&source.canonicalize()?)?.identity;
        anyhow::ensure!(
            !ancestors.contains(&resolved),
            "不能将文件夹移入自身或子目录"
        );
    }
    Ok(())
}
pub fn validate_ledger(
    ledger: &super::ledger::Ledger,
    target: &Target,
    canceled: impl Fn() -> bool,
) -> anyhow::Result<()> {
    target.verify()?;
    let chain = ancestors(&target.path)?;
    ledger.visit_roots(false, |root| {
        anyhow::ensure!(!canceled(), "已取消");
        let now = identity::capture(&root.path)?;
        anyhow::ensure!(
            root.identity.as_ref() == Some(&now.identity),
            "源文件已改变，请重新选择"
        );
        if now.directory || now.reparse {
            check(&root.path, &chain)?;
        }
        Ok(())
    })?;
    target.verify()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descendant_cycle_is_rejected_and_target_replacement_invalidates_drop() {
        let f = crate::test_support::Fixture::new("move-guard");
        let source = f.0.join("tree");
        let child = source.join("child");
        let sibling = f.0.join("tree-other");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        assert!(check(&source, &ancestors(&child).unwrap()).is_err());
        assert!(check(&source, &ancestors(&sibling).unwrap()).is_ok());
        let target = Target::capture(sibling.clone()).unwrap();
        std::fs::rename(&sibling, f.0.join("preserved-target")).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        assert!(target.verify().is_err());
        assert!(source.exists());
    }
}
