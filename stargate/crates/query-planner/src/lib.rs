#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate derive_builder;

use crate::builder::build_query_plan;
use crate::model::QueryPlan;
use graphql_parser::{parse_query, parse_schema, schema, ParseError};
use serde::{Deserialize, Serialize};

// This is the interface to the JVM that we'll call the majority of our
// methods on.
use jni::JNIEnv;

// These objects are what you should use as arguments to your native
// function. They carry extra lifetime information to prevent them escaping
// this context and getting used after being GC'd.
use jni::objects::{JClass, JString};

// This is just a pointer. We'll be returning it from our function. We
// can't return one of the objects with lifetime information because the
// lifetime checker won't let us.
use jni::sys::jstring;

use serde_json::json;

#[macro_use]
mod macros;
mod autofrag;
mod builder;
mod consts;
mod context;
mod federation;
mod groups;
pub mod helpers;
pub mod model;
mod visitors;

// This keeps Rust from "mangling" the name and making it unique for this
// crate.
#[no_mangle]
#[allow(unused_variables)]
pub extern "system" fn Java_HelloWorld_hello(env: JNIEnv,
// This is the class that owns our static method. It's not going to be used,
// but still must be present to match the expected signature of a static
// native method.
                                             class: JClass,
                                             input: JString)
                                             -> jstring {
    // First, we have to get the string out of Java. Check out the `strings`
    // module for more info on how this works.
    let input: String =
        env.get_string(input).expect("Couldn't get java string!").into();

    // Then we have to create a new Java string to return. Again, more info
    // in the `strings` module.
    //let output = env.new_string(format!("Hello, {}!", input))
        //.expect("Couldn't create java string!");

    let planner = QueryPlanner::new(&input);
    let query = "query {
      me {
        name
      }
    }";
    let options = QueryPlanningOptionsBuilder::default()
        .build()
        .unwrap();
    let result = planner.plan(query, options).expect("Couldn't create java string!");

    let outcome = json!(result);

    let output = env.new_string(format!("Hello, {}!", outcome))
        .expect("Couldn't create java string!");

    // Finally, extract the raw pointer to return.
    output.into_inner()
}

#[derive(Debug)]
pub enum QueryPlanError {
    FailedParsingSchema(ParseError),
    FailedParsingQuery(ParseError),
    InvalidQuery(&'static str),
}

pub type Result<T> = std::result::Result<T, QueryPlanError>;

#[derive(Debug)]
pub struct QueryPlanner<'s> {
    pub schema: schema::Document<'s>,
}

impl<'s> QueryPlanner<'s> {
    pub fn new(schema: &'s str) -> QueryPlanner<'s> {
        let schema = parse_schema(schema).expect("failed parsing schema");
        QueryPlanner { schema }
    }

    // TODO(ran) FIXME: make options a field on the planner.
    pub fn plan(&self, query: &str, options: QueryPlanningOptions) -> Result<QueryPlan> {
        let query = parse_query(query).expect("failed parsing query");
        build_query_plan(&self.schema, &query, options)
    }
}

// NB: By deriving Builder (using the derive_builder crate) we automatically implement
// the builder pattern for arbitrary structs.
// simple #[derive(Builder)] will generate a FooBuilder for your struct Foo with all setter-methods and a build method.
#[derive(Default, Builder, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlanningOptions {
    #[builder(default)]
    auto_fragmentization: bool,
}

#[cfg(test)]
mod tests {
    use crate::model::QueryPlan;
    use crate::{QueryPlanner, QueryPlanningOptionsBuilder};
    use gherkin_rust::Feature;
    use gherkin_rust::StepType;
    use std::fs::{read_dir, read_to_string};
    use std::path::PathBuf;

    macro_rules! get_step {
        ($scenario:ident, $typ:pat) => {
            $scenario
                .steps
                .iter()
                .find(|s| matches!(s.ty, $typ))
                .unwrap()
                .docstring
                .as_ref()
                .unwrap()
        };
    }

    /// This test looks over all directorys under tests/features and finds "csdl.graphql" in
    /// each of those directories. It runs all of the .feature cases in that directory against that schema.
    /// To add test cases against new schemas, create a sub directory under "features" with the new schema
    /// and new .feature files.
    #[test]
    fn test_all_feature_files() {
        // If debugging with IJ, use `read_dir("query-planner/tests/features")`
        // let dirs = read_dir("query-planner/tests/features")
        let dirs = read_dir(PathBuf::from("tests").join("features"))
            .unwrap()
            .map(|res| res.map(|e| e.path()).unwrap())
            .filter(|d| d.is_dir());

        for dir in dirs {
            let schema = read_to_string(dir.join("csdl.graphql")).unwrap();
            let planner = QueryPlanner::new(&schema);
            let feature_paths = read_dir(dir)
                .unwrap()
                .map(|res| res.map(|e| e.path()).unwrap())
                .filter(|e| {
                    if let Some(d) = e.extension() {
                        d == "feature"
                    } else {
                        false
                    }
                });

            for path in feature_paths {
                let feature = read_to_string(&path).unwrap();

                let feature = match Feature::parse(feature) {
                    Result::Ok(feature) => feature,
                    Result::Err(e) => panic!("Unparseable .feature file {:?} -- {}", &path, e),
                };

                for scenario in feature.scenarios {
                    let query: &str = get_step!(scenario, StepType::Given);
                    let expected_str: &str = get_step!(scenario, StepType::Then);
                    let expected: QueryPlan = serde_json::from_str(&expected_str).unwrap();

                    let auto_fragmentization = scenario
                        .steps
                        .iter()
                        .any(|s| matches!(s.ty, StepType::When));
                    let options = QueryPlanningOptionsBuilder::default()
                        .auto_fragmentization(auto_fragmentization)
                        .build()
                        .unwrap();
                    let result = planner.plan(query, options).unwrap();
                    assert_eq!(result, expected);
                }
            }
        }
    }

    #[test]
    fn query_planning_options_initialization() {
        let options = QueryPlanningOptionsBuilder::default().build().unwrap();
        assert_eq!(false, options.auto_fragmentization);
    }
}
