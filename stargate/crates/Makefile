java_run: lib
	javac HelloWorld.java && java -Djava.library.path=query-planner/target/debug/ HelloWorld

.PHONY: lib

javah:
	javac -h . HelloWorld.java

lib:
	cd query-planner && cargo build --target-dir target
