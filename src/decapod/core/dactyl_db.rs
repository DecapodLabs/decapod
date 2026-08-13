//! Decapod's application-facing storage facade.
//!
//! This module is the only compatibility surface between Decapod's existing
//! relational call sites and Dactyl.  Decapod owns schemas, migration order,
//! retry policy, and domain semantics; Dactyl owns physical execution,
//! binding, access mode, atomicity, and normalized results.

use dactyl_db::{AccessMode, DactylError, DatastoreRoute, OpenOptions, Parameter};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Null,
    Integer,
    Real,
    Text,
    Blob,
}

#[derive(Debug)]
pub enum Error {
    Dactyl(DactylError),
    QueryReturnedNoRows,
    FromSqlConversionFailure(usize, Type, Box<dyn std::error::Error + Send + Sync>),
    ToSqlConversionFailure(Box<dyn std::error::Error + Send + Sync>),
    InvalidColumnType(usize, String, Type),
    InvalidParameterName(String),
    InvalidQuery,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dactyl(error) => write!(f, "{error}"),
            Self::QueryReturnedNoRows => f.write_str("query returned no rows"),
            Self::FromSqlConversionFailure(index, kind, error) => {
                write!(f, "column {index} conversion from {kind:?} failed: {error}")
            }
            Self::ToSqlConversionFailure(error) => {
                write!(f, "parameter conversion failed: {error}")
            }
            Self::InvalidColumnType(index, name, kind) => {
                write!(f, "invalid column type at {index} ({name}): {kind:?}")
            }
            Self::InvalidParameterName(name) => write!(f, "invalid parameter name: {name}"),
            Self::InvalidQuery => f.write_str("invalid query"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dactyl(error) => Some(error),
            Self::FromSqlConversionFailure(_, _, error) => Some(error.as_ref()),
            Self::ToSqlConversionFailure(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<DactylError> for Error {
    fn from(error: DactylError) -> Self {
        Self::Dactyl(error)
    }
}

pub trait ToSql: Send + Sync {
    fn to_parameter(&self) -> Parameter;
}

impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_parameter(&self) -> Parameter {
        (*self).to_parameter()
    }
}

impl<T: ToSql + ?Sized> ToSql for Box<T> {
    fn to_parameter(&self) -> Parameter {
        (**self).to_parameter()
    }
}

macro_rules! impl_to_sql {
    ($ty:ty, $expr:expr) => {
        impl ToSql for $ty {
            fn to_parameter(&self) -> Parameter {
                $expr(self)
            }
        }
    };
}

impl_to_sql!(i64, |value: &i64| Parameter::Integer(*value));
impl_to_sql!(i32, |value: &i32| Parameter::Integer(i64::from(*value)));
impl_to_sql!(i16, |value: &i16| Parameter::Integer(i64::from(*value)));
impl_to_sql!(i8, |value: &i8| Parameter::Integer(i64::from(*value)));
impl_to_sql!(u64, |value: &u64| Parameter::Integer(*value as i64));
impl_to_sql!(u32, |value: &u32| Parameter::Integer(i64::from(*value)));
impl_to_sql!(u16, |value: &u16| Parameter::Integer(i64::from(*value)));
impl_to_sql!(u8, |value: &u8| Parameter::Integer(i64::from(*value)));
impl_to_sql!(usize, |value: &usize| Parameter::Integer(*value as i64));
impl_to_sql!(isize, |value: &isize| Parameter::Integer(*value as i64));
impl_to_sql!(f64, |value: &f64| Parameter::Real(*value));
impl_to_sql!(f32, |value: &f32| Parameter::Real(f64::from(*value)));
impl_to_sql!(bool, |value: &bool| Parameter::Bool(*value));
impl_to_sql!(String, |value: &String| Parameter::Text(value.clone()));
impl_to_sql!(&str, |value: &&str| Parameter::Text((*value).to_owned()));
impl<'a> ToSql for Cow<'a, str> {
    fn to_parameter(&self) -> Parameter {
        Parameter::Text(self.to_string())
    }
}
impl_to_sql!(Vec<u8>, |value: &Vec<u8>| Parameter::Blob(value.clone()));
impl_to_sql!(&[u8], |value: &&[u8]| Parameter::Blob((*value).to_vec()));

impl<T: ToSql> ToSql for Option<T> {
    fn to_parameter(&self) -> Parameter {
        match self {
            Some(value) => value.to_parameter(),
            None => Parameter::Null,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Params(Vec<Parameter>);

impl Params {
    fn into_inner(self) -> Vec<Parameter> {
        self.0
    }
}

pub trait IntoParams {
    fn into_params(self) -> Vec<Parameter>;
}

impl IntoParams for Params {
    fn into_params(self) -> Vec<Parameter> {
        self.into_inner()
    }
}

impl IntoParams for () {
    fn into_params(self) -> Vec<Parameter> {
        Vec::new()
    }
}

impl IntoParams for [(); 0] {
    fn into_params(self) -> Vec<Parameter> {
        Vec::new()
    }
}

macro_rules! impl_array_params {
    ($($length:expr),+ $(,)?) => {
        $(
            impl<T: ToSql> IntoParams for [T; $length] {
                fn into_params(self) -> Vec<Parameter> {
                    self.into_iter().map(|value| value.to_parameter()).collect()
                }
            }
        )+
    };
}

impl_array_params!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

impl<T: ToSql> IntoParams for Vec<T> {
    fn into_params(self) -> Vec<Parameter> {
        self.into_iter().map(|value| value.to_parameter()).collect()
    }
}

impl<T: ToSql> IntoParams for &[T] {
    fn into_params(self) -> Vec<Parameter> {
        self.iter().map(ToSql::to_parameter).collect()
    }
}

impl<T: ToSql> IntoParams for &Vec<T> {
    fn into_params(self) -> Vec<Parameter> {
        self.iter().map(ToSql::to_parameter).collect()
    }
}

pub fn params_from_iter<I, T>(iter: I) -> Params
where
    I: IntoIterator<Item = T>,
    T: ToSql,
{
    Params(iter.into_iter().map(|value| value.to_parameter()).collect())
}

pub fn params_from_values(values: Vec<Parameter>) -> Params {
    Params(values)
}

#[macro_export]
macro_rules! params {
    ($($value:expr),* $(,)?) => {
        $crate::core::db::params_from_values(vec![$($crate::core::db::ToSql::to_parameter(&$value)),*])
    };
}

pub trait FromSql: Sized {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self>;
}

impl FromSql for String {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Text))
    }
}

impl FromSql for i64 {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        value
            .as_i64()
            .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Integer))
    }
}

impl FromSql for i32 {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        i64::from_value(index, value).and_then(|value| {
            i32::try_from(value).map_err(|error| {
                Error::FromSqlConversionFailure(index, Type::Integer, Box::new(error))
            })
        })
    }
}

impl FromSql for u64 {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        i64::from_value(index, value).and_then(|value| {
            u64::try_from(value).map_err(|error| {
                Error::FromSqlConversionFailure(index, Type::Integer, Box::new(error))
            })
        })
    }
}

impl FromSql for usize {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        u64::from_value(index, value).and_then(|value| {
            usize::try_from(value).map_err(|error| {
                Error::FromSqlConversionFailure(index, Type::Integer, Box::new(error))
            })
        })
    }
}

impl FromSql for f64 {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|value| value as f64))
            .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Real))
    }
}

impl FromSql for bool {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        value
            .as_bool()
            .or_else(|| value.as_i64().map(|value| value != 0))
            .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Integer))
    }
}

impl FromSql for Vec<u8> {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        value
            .as_array()
            .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Blob))?
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or_else(|| Error::InvalidColumnType(index, value.to_string(), Type::Blob))
            })
            .collect()
    }
}

impl FromSql for JsonValue {
    fn from_value(_index: usize, value: &JsonValue) -> Result<Self> {
        Ok(value.clone())
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_value(index: usize, value: &JsonValue) -> Result<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_value(index, value).map(Some)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFlags(u32);

impl OpenFlags {
    pub const SQLITE_OPEN_READ_ONLY: Self = Self(0x01);
    pub const SQLITE_OPEN_READ_WRITE: Self = Self(0x02);
    pub const SQLITE_OPEN_CREATE: Self = Self(0x04);
    pub const SQLITE_OPEN_NO_MUTEX: Self = Self(0x08);

    fn access_mode(self) -> AccessMode {
        if self.0 & Self::SQLITE_OPEN_READ_ONLY.0 != 0 {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        }
    }
}

impl std::ops::BitOr for OpenFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

pub struct Connection {
    driver: Rc<dactyl_db::Connection>,
    transaction_state: Rc<Cell<bool>>,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Connection {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_options(path, AccessMode::ReadWrite, Duration::from_secs(5))
    }

    pub fn open_with_flags(path: impl AsRef<Path>, flags: OpenFlags) -> Result<Self> {
        Self::open_with_options(path, flags.access_mode(), Duration::from_secs(5))
    }

    pub fn open_with_options(
        path: impl AsRef<Path>,
        access_mode: AccessMode,
        lock_timeout: Duration,
    ) -> Result<Self> {
        // Dactyl v0.8.2 validates an existing local file header before it
        // reaches SQLite's CREATE flag. Seed the empty file for a new
        // read-write datastore so the Dactyl open remains the authority for
        // the actual connection and header validation.
        ensure_dactyl_create_target(path.as_ref(), access_mode);
        let driver = dactyl_db::Connection::open_with_options(
            DatastoreRoute::sqlite(path.as_ref().to_string_lossy().into_owned()),
            OpenOptions {
                access_mode,
                lock_timeout,
            },
        )?;
        Ok(Self {
            driver: Rc::new(driver),
            transaction_state: Rc::new(Cell::new(false)),
        })
    }

    pub fn busy_timeout(&self, _timeout: Duration) -> Result<()> {
        Ok(())
    }

    pub fn execute<P: IntoParams>(&self, sql: &str, params: P) -> Result<usize> {
        Ok(self.driver.write(sql, &params.into_params())? as usize)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        for statement in split_sql_statements(sql) {
            if statement.trim().is_empty() {
                continue;
            }
            let keyword = first_keyword(&statement);
            self.driver.write(&statement, &[])?;
            match keyword.as_deref() {
                Some("begin") => self.set_transaction_state(true),
                Some("commit") | Some("rollback") | Some("end") => {
                    self.set_transaction_state(false)
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn prepare<'conn>(&'conn self, sql: &str) -> Result<Statement<'conn>> {
        Ok(Statement {
            connection: self,
            sql: sql.to_owned(),
        })
    }

    pub fn query_row<P, F, T>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: IntoParams,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        self.prepare(sql)?.query_row(params, f)
    }

    pub fn transaction(&self) -> Result<Transaction<'_>> {
        let owner = !self.is_in_transaction();
        if owner {
            self.execute_batch("BEGIN IMMEDIATE")?;
        }
        Ok(Transaction {
            connection: self,
            owner,
            active: true,
        })
    }

    pub fn is_autocommit(&self) -> bool {
        !self.is_in_transaction()
    }

    pub fn inspect_schema(&self) -> Result<dactyl_db::StoreSchema> {
        Ok(self.driver.inspect_schema()?)
    }

    pub fn has_table(&self, table: &str) -> Result<bool> {
        Ok(self.inspect_schema()?.table(table).is_some())
    }

    pub fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        Ok(self
            .inspect_schema()?
            .table(table)
            .is_some_and(|table| table.columns.iter().any(|item| item.name == column)))
    }

    pub fn columns(&self, table: &str) -> Result<Vec<String>> {
        Ok(self
            .inspect_schema()?
            .table(table)
            .map(|table| {
                table
                    .columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    fn is_in_transaction(&self) -> bool {
        self.transaction_state.get()
    }

    fn set_transaction_state(&self, active: bool) {
        self.transaction_state.set(active);
    }
}

fn ensure_dactyl_create_target(path: &Path, access_mode: AccessMode) {
    if access_mode != AccessMode::ReadWrite || path == Path::new(":memory:") || path.exists() {
        return;
    }

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path);
}

pub struct Statement<'conn> {
    connection: &'conn Connection,
    sql: String,
}

impl<'conn> Statement<'conn> {
    pub fn execute<P: IntoParams>(&mut self, params: P) -> Result<usize> {
        self.connection.execute(&self.sql, params)
    }

    pub fn query<P: IntoParams>(&mut self, params: P) -> Result<Rows> {
        let rows = self
            .connection
            .driver
            .read(&self.sql, &params.into_params())?;
        Ok(Rows::new(rows))
    }

    pub fn query_map<P, F, T>(&mut self, params: P, f: F) -> Result<MappedRows<T>>
    where
        P: IntoParams,
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let rows = self.query(params)?;
        Ok(rows.map(f))
    }

    pub fn query_row<P, F, T>(&mut self, params: P, f: F) -> Result<T>
    where
        P: IntoParams,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        let mut rows = self.query(params)?;
        match rows.next()? {
            Some(row) => f(&row),
            None => Err(Error::QueryReturnedNoRows),
        }
    }
}

pub struct Rows {
    rows: Vec<OwnedRow>,
    position: usize,
}

impl Rows {
    fn new(rows: dactyl_db::Rows) -> Self {
        Self {
            rows: rows.0.into_iter().map(OwnedRow).collect(),
            position: 0,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Row<'_>>> {
        let Some(row) = self.rows.get(self.position) else {
            return Ok(None);
        };
        self.position += 1;
        Ok(Some(Row { row: &row.0 }))
    }
}

struct OwnedRow(dactyl_db::Row);

pub struct Row<'row> {
    row: &'row dactyl_db::Row,
}

impl<'row> Row<'row> {
    pub fn get<I, T>(&self, index: I) -> Result<T>
    where
        I: RowIndex,
        T: FromSql,
    {
        let (index, value) = index.value(self.row)?;
        T::from_value(index, value)
    }

    pub fn parameters(&self) -> Vec<Parameter> {
        self.row.values.iter().map(value_to_parameter).collect()
    }
}

fn value_to_parameter(value: &JsonValue) -> Parameter {
    match value {
        JsonValue::Null => Parameter::Null,
        JsonValue::Bool(value) => Parameter::Bool(*value),
        JsonValue::Number(value) => value
            .as_i64()
            .map(Parameter::Integer)
            .or_else(|| value.as_f64().map(Parameter::Real))
            .unwrap_or(Parameter::Null),
        JsonValue::String(value) => Parameter::Text(value.clone()),
        JsonValue::Array(values) => Parameter::Blob(
            values
                .iter()
                .filter_map(|value| value.as_u64().and_then(|value| u8::try_from(value).ok()))
                .collect(),
        ),
        JsonValue::Object(_) => Parameter::Text(value.to_string()),
    }
}

pub trait RowIndex {
    fn value<'row>(&self, row: &'row dactyl_db::Row) -> Result<(usize, &'row JsonValue)>;
}

impl RowIndex for usize {
    fn value<'row>(&self, row: &'row dactyl_db::Row) -> Result<(usize, &'row JsonValue)> {
        row.values
            .get(*self)
            .map(|value| (*self, value))
            .ok_or_else(|| Error::InvalidColumnType(*self, String::new(), Type::Null))
    }
}

impl RowIndex for &str {
    fn value<'row>(&self, row: &'row dactyl_db::Row) -> Result<(usize, &'row JsonValue)> {
        let index = row
            .columns
            .iter()
            .position(|column| column == self)
            .ok_or_else(|| Error::InvalidParameterName((*self).to_owned()))?;
        Ok((index, &row.values[index]))
    }
}

impl RowIndex for String {
    fn value<'row>(&self, row: &'row dactyl_db::Row) -> Result<(usize, &'row JsonValue)> {
        self.as_str().value(row)
    }
}

pub struct MappedRows<T> {
    values: std::vec::IntoIter<Result<T>>,
}

impl<T> MappedRows<T> {
    fn new(values: Vec<Result<T>>) -> Self {
        Self {
            values: values.into_iter(),
        }
    }
}

impl<T> Iterator for MappedRows<T> {
    type Item = Result<T>;
    fn next(&mut self) -> Option<Self::Item> {
        self.values.next()
    }
}

impl Rows {
    fn map<T, F>(mut self, mut f: F) -> MappedRows<T>
    where
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let mut values = Vec::with_capacity(self.rows.len());
        while let Ok(Some(row)) = self.next() {
            values.push(f(&row));
        }
        MappedRows::new(values)
    }
}

pub struct Transaction<'conn> {
    connection: &'conn Connection,
    owner: bool,
    active: bool,
}

impl std::ops::Deref for Transaction<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
    }
}

impl<'conn> Transaction<'conn> {
    pub fn execute<P: IntoParams>(&self, sql: &str, params: P) -> Result<usize> {
        self.connection.execute(sql, params)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.connection.execute_batch(sql)
    }

    pub fn prepare<'tx>(&'tx self, sql: &str) -> Result<Statement<'tx>> {
        self.connection.prepare(sql)
    }

    pub fn query_row<P, F, T>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: IntoParams,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        self.connection.query_row(sql, params, f)
    }

    pub fn commit(mut self) -> Result<()> {
        if self.owner && self.active {
            self.connection.execute_batch("COMMIT")?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.owner && self.active {
            let _ = self.connection.execute_batch("ROLLBACK");
            self.active = false;
        }
    }
}

pub trait OptionalExtension<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExtension<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

pub mod types {
    pub use super::{ToSql, Type};
}

fn first_keyword(sql: &str) -> Option<String> {
    sql.split_whitespace().next().map(|word| {
        word.trim_matches(|c: char| !c.is_ascii_alphabetic())
            .to_ascii_lowercase()
    })
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let bytes = sql.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        match quote {
            Some(delimiter) if *byte == delimiter => quote = None,
            Some(_) => {}
            None if *byte == b'\'' || *byte == b'"' || *byte == b'`' => quote = Some(*byte),
            None if *byte == b';' => {
                statements.push(sql[start..index].trim().to_owned());
                start = index + 1;
            }
            None => {}
        }
    }
    if !sql[start..].trim().is_empty() {
        statements.push(sql[start..].trim().to_owned());
    }
    statements
}
