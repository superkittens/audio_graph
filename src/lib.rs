

pub mod AudioToolbox {

    pub struct Error {
        pub code: ErrorCodes,
        pub message: String
    }

    pub enum ErrorCodes {
        node_id_non_existent,
        node_input_port_invalid,
        node_no_more_inputs,
        node_parent_already_exists,
        node_connecting_to_itself,
        output_node_has_no_inputs,
        connection_already_exists,
        invalid_buffer_size,
        invalid_sampling_frequency
    }

    pub trait AudioNode {
        fn init(&mut self, audio_runtime_params: &AudioRuntimeParameters) {}
        fn get_node_type(&self) -> &AudioNodeType;
        fn get_number_of_inputs(&self) -> usize;
        fn get_next_available_input(&self) -> Option<usize>;
        fn connect_input(&mut self) {}
        fn change_parameters<'a>(&mut self, parameters: &'a [f32]) {}
        fn reset(&mut self) {}
        fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] { buffer }
    }

    pub enum AudioNodeType {
        Test,
        Generator,
        Effect,
        Mixer,
        Output,
        Unknown
    }

    /// An output node instance
    /// This is a private struct since the library user should only concern themselves with making/adding generator, mixer or effect nodes
    struct OutputNode {
        node_type: AudioNodeType,
        num_inputs: usize,
        next_available_input: usize
    }

    impl AudioNode for OutputNode {
        fn get_node_type(&self) -> &AudioNodeType {
            &self.node_type
        }

        fn get_number_of_inputs(&self) -> usize {
            self.num_inputs
        }

        fn get_next_available_input(&self) -> Option<usize> {
            if self.next_available_input < self.num_inputs {
                return Some(self.next_available_input);
            }

            None
        }

        fn connect_input(&mut self) {
            if self.next_available_input <= self.num_inputs {
                self.next_available_input += 1
            }
        }
    }

    impl OutputNode {
        fn new() -> OutputNode {
            OutputNode {
                node_type: AudioNodeType::Output,
                num_inputs: 1,
                next_available_input: 0
            }
        }
    }

    
    /// Audio Runtime Parameters
    /// This struct carries information about audio playback settings such as sampling frequency and buffer size
    /// An instance of this struct is passed to AudioGraph::prepare() before the audio graph is run (if not, default parameters are used)
    pub struct AudioRuntimeParameters {
        pub sampling_freq: f32,
        pub buffer_size: usize,
    }


    /// GraphTree is a map that keeps track of the connections between different nodes
    /// The AudioNode instances themselves do not track the relationships between other nodes.  That is the job of the GraphTree
    /// Each node that is added to the graph is assigned an identification, which is also the index of the nodes vector in this struct
    /// NOTE:  The 0th index is ALWAYS reserved for the output node (and is also the root of the node tree)
    struct NodeTree {
        nodes: Vec<MapNode>
    }

    /// MapNode is a "leaf" structure that belongs to a NodeTree instance
    /// When an AudioNode is added to the AudioGraph, a corresponding MapNode is created and added to the node tree
    struct MapNode {
        parent: Option<usize>,
        children: Vec<usize>,
    }

    impl MapNode {
        fn new() -> MapNode {
            MapNode {
                parent: None,
                children: vec![],
            }
        }
    }



    /// Audio Graph
    /// An audio graph is responsible for creating and modifying audio samples that eventually are written to some output buffer
    /// Sample creation and modification is handled by the individual nodes 
    /// When get_samples() is called, the graph will traverse through each node, where each node will either place samples (if a generator node) or modify it (effect node)
    pub struct AudioGraph {
        nodes: Vec<Box<dyn AudioNode + 'static>>,
        graph_map: NodeTree,
        iter_stack: Vec<(usize, usize, usize)>,
        iter_stack_size: usize,
        audio_runtime_params: AudioRuntimeParameters
    }


    impl AudioGraph {
        /// Create a new audio graph instance
        pub fn new() -> AudioGraph {
            AudioGraph {
                nodes: vec![Box::new(OutputNode::new())],
                graph_map: NodeTree {
                    nodes: vec![MapNode {parent: None, children: vec![]}]
                },
                iter_stack: vec![(0, 0, 0)],
                iter_stack_size: 0,
                audio_runtime_params: AudioRuntimeParameters {
                                            sampling_freq: 44_100.0,
                                            buffer_size: 512
                }
            }
        }

        /// Add an AudioNode to the graph
        /// NOTE that calling this function will NOT establish any connections to other nodes.  It simply adds the node to the ownership list, nodes
        /// This function will return an identification number that the user can then use to reference the added node when making connections/disconnections
        pub fn add_new_node(&mut self, n: Box<dyn AudioNode + 'static>) -> Option<usize> {
            match n.get_node_type() {
                AudioNodeType::Output => { return None; },
                _ => {}
            }
            
            self.nodes.push(n);
            self.graph_map.nodes.push(MapNode::new());
            self.iter_stack.push((0, 0, 0));

            Some(self.nodes.len() - 1)
        }

        pub fn connect_node_to_output(&mut self, node_out_id: usize) -> Result<(), Error> {
            match self.nodes[0].get_next_available_input() {
                Some(i) => self.connect_node(node_out_id, 0, i),
                None => Err(Error {
                                code: ErrorCodes::output_node_has_no_inputs,
                                message: String::from("Output node already has a connection to a child node")
                })
            }
        }

        /// Connect two nodes together
        /// The output of node1 is connected to the input of node2
        /// Specify which input of node_in that the output of node_out will be connected to
        /// 
        /// [node_out]->[node_in]
        pub fn connect_node(&mut self, node_out_id: usize, node_in_id: usize, node_in_input_port: usize) -> Result<(), Error> {

            let validation_result = self.validate_node_inputs(node_out_id, node_in_id, node_in_input_port);
            match validation_result {
                Err(e) => {return Err(e);},
                Ok(()) => {
                    // Find the associated MapNodes in the Tree and set parent/children
                    // You cannot assign more than one parent for a given node 
                    match self.graph_map.nodes[node_out_id].parent {
                        None => {
                            self.graph_map.nodes[node_in_id].children.push(node_out_id);
                            self.nodes[node_in_id].connect_input();

                            self.graph_map.nodes[node_out_id].parent = Some(node_in_id);
                        }

                        Some(_i) => {return Err(Error {
                                        code: ErrorCodes::node_parent_already_exists,
                                        message: String::from("node_out already has a parent: ")
                                    });
                                    }
                        }
                    }
            }

            Ok(())
        }

        fn validate_node_inputs(&self, node_out_id: usize, node_in_id: usize, node_in_input_port: usize) -> Result<(), Error> {
            //  Make sure node actually exists in graph
            if node_in_id > self.nodes.len() || node_out_id > self.nodes.len() {
                return Err(Error{
                    code: ErrorCodes::node_id_non_existent,
                    message: String::from("Node ID does not exist in graph")
                });
            }

            //  Ensure that a valid input port is passed in
            if node_in_input_port >= self.nodes[node_in_id].get_number_of_inputs() {
                return Err(Error {
                    code: ErrorCodes::node_input_port_invalid,
                    message: String::from("Node input port not valid")
                });
            }

            //  Ensure that unconnected inputs are actually available
            if self.nodes[node_in_id].get_next_available_input() == None {
                return Err(Error {
                    code: ErrorCodes::node_no_more_inputs,
                    message: String::from("Input node has no more available inputs")
                });
            }

            //  Ensure that a node is not being connected to itself
            if node_out_id == node_in_id {
                return Err(Error {
                    code: ErrorCodes::node_connecting_to_itself,
                    message: String::from("Cannot connect a node to itself")
                });
            }

            //  Check to make sure that the connection doesn't already exist
            for child in &self.graph_map.nodes[node_out_id].children {
                if *child == node_in_id {
                    return Err( Error {
                        code: ErrorCodes::connection_already_exists,
                        message: String::from("The node connection already exists")
                    });
                }
            }

            Ok(())
        }

        /// Prepare the audio graph with a specified set of audio runtime parameters (sampling freq, buffer size etc)
        /// This function will call the initialization functions for all of the nodes
        pub fn prepare(&mut self, audio_parameters: AudioRuntimeParameters) -> Result<(), Error> {
            //  Check that audio_parameters have valid inputs
            if audio_parameters.buffer_size == 0 {
                return Err(Error {
                    code: ErrorCodes::invalid_buffer_size,
                    message: String::from("Invalid buffer size")
                });
            }

            if audio_parameters.sampling_freq <= 0.0 {
                return Err( Error {
                    code: ErrorCodes::invalid_sampling_frequency,
                    message: String::from("Invalid sampling frequency entered")
                });
            }

            self.audio_runtime_params = audio_parameters;

            for node in &mut self.nodes {
                node.init(&self.audio_runtime_params);
            }

            Ok(())
        }

        /// Run the audio graph and get a buffer of samples
        pub fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] {
            //  First initialize the stack used for graph traversal
            self.go_to_branch_end(0);

            while let Some(node) = self.next() {
                self.nodes[node].process_block(buffer);
                println!("Node: {}", node); //  For debugging
            }

            buffer
        }


        //  Functions for iterating through the graph node
        //  ==============================================================================================================  //
        /// For a given node, go to the end of its branch and push the intermediate nodes onto the stack
        fn go_to_branch_end(&mut self, node_id: usize) {
            let mut current_id = node_id;
            loop {
                self.iter_stack[self.iter_stack_size] = (current_id, self.graph_map.nodes[current_id].children.len(), 0);
                self.iter_stack_size += 1;

                if self.graph_map.nodes[current_id].children.len() > 0 {
                    current_id = self.graph_map.nodes[current_id].children[0];
                } else {
                    break;
                }
            }
        }

        /// Retrieve the next node id in the audio graph chain.  Will return None if the entire graph has been processed
        fn next(&mut self) -> Option<usize> {
            if self.iter_stack_size > 0 {
                //  First, check to see if there are any unvisited children nodes in the current node
                //  If there are, then traverse to the end of the branch and push the nodes along the way onto the stack
                let (node_id, num_children, child_index) = self.iter_stack[self.iter_stack_size - 1];
                if child_index + 1 < num_children {
                    //  Mark child as visited
                    self.iter_stack[self.iter_stack_size].2 = child_index + 1;

                    let child_node_id = self.graph_map.nodes[node_id].children[child_index + 1];
                    self.go_to_branch_end(child_node_id);
                }

                let next_node_id = self.iter_stack[self.iter_stack_size - 1].0;
                self.iter_stack_size -= 1;

                return Some(next_node_id);
            }
            
            None
        }
    }
}


mod TestNodes {
    use super::AudioToolbox::{AudioNodeType, AudioNode, AudioRuntimeParameters};

    pub struct TestNode {
        data: i32,
        node_type: AudioNodeType,
        num_inputs: usize,
        inputs: [i32; 1],
        next_available_input: usize
    }

    impl AudioNode for TestNode {

        fn get_node_type(&self) -> &AudioNodeType {
            &self.node_type
        }

        fn get_number_of_inputs(&self) -> usize {
            self.num_inputs
        }

        fn get_next_available_input(&self) -> Option<usize> {
            if self.next_available_input >= self.num_inputs {
                return None;
            } 

            Some(self.next_available_input)
        }

        fn connect_input(&mut self) {
            if self.next_available_input < self.num_inputs {
                self.next_available_input += 1;
            }
        }

        fn reset(&mut self) {
            self.data = 88;
        }

        fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] {
            buffer
        }
    }

    impl TestNode {
        pub fn new() -> TestNode {
            TestNode {
                data: 88,
                node_type: AudioNodeType::Test,
                num_inputs: 1,
                inputs: [-1; 1],
                next_available_input: 0
            }
        }
    }


    pub struct TestGenNode {
        node_type: AudioNodeType,
        audio_runtime_params: AudioRuntimeParameters
    }

    impl AudioNode for TestGenNode {
        fn init(&mut self, audio_runtime_params: &AudioRuntimeParameters) {
            println!("Preparing generator node with parameters, fs: {}, buffer_size: {}", audio_runtime_params.sampling_freq, audio_runtime_params.buffer_size);
            self.audio_runtime_params.buffer_size = audio_runtime_params.buffer_size;
            self.audio_runtime_params.sampling_freq = audio_runtime_params.sampling_freq;
        }

        fn get_node_type(&self) -> &AudioNodeType {
            &self.node_type
        }

        fn get_number_of_inputs(&self) -> usize {
            0
        }

        fn get_next_available_input(&self) -> Option<usize> {
            None
        }

        fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] {
            if self.audio_runtime_params.buffer_size > 0 {
                for i in 0..self.audio_runtime_params.buffer_size {
                    buffer[i] = 1.0;
                }
            }

            buffer
        }
    }

    impl TestGenNode {
        pub fn new() -> TestGenNode {
            TestGenNode {
                node_type: AudioNodeType::Generator,
                audio_runtime_params: AudioRuntimeParameters {
                    sampling_freq: 0.0,
                    buffer_size: 0
                }
            }
        }
    }

    pub struct TestFXNode {
        node_type: AudioNodeType,
        inputs: [i32; 1],
        num_inputs: usize,
        next_available_input: usize,
        audio_runtime_params: AudioRuntimeParameters
    }

    impl AudioNode for TestFXNode {
        fn init(&mut self, audio_runtime_params: &AudioRuntimeParameters) {
            println!("Preparing generator node with parameters, fs: {}, buffer_size: {}", audio_runtime_params.sampling_freq, audio_runtime_params.buffer_size);
            self.audio_runtime_params.buffer_size = audio_runtime_params.buffer_size;
            self.audio_runtime_params.sampling_freq = audio_runtime_params.sampling_freq;
        }

        fn get_node_type(&self) -> &AudioNodeType {
            &self.node_type
        }

        fn get_number_of_inputs(&self) -> usize {
            1
        }

        fn get_next_available_input(&self) -> Option<usize> {
            if self.next_available_input >= self.num_inputs {
                return None;
            } 

            Some(self.next_available_input)
        }

        fn connect_input(&mut self) {
            if self.next_available_input < self.num_inputs {
                self.next_available_input += 1;
            }
        }

        fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] {
            if self.audio_runtime_params.buffer_size > 0 {
                for i in 0..self.audio_runtime_params.buffer_size {
                    buffer[i] *= 0.5;
                }
            }

            buffer
        }
    }

    impl TestFXNode {
        pub fn new() -> TestFXNode {
            TestFXNode {
                node_type: AudioNodeType::Effect,
                inputs: [-1; 1],
                num_inputs: 1,
                next_available_input: 0,
                audio_runtime_params: AudioRuntimeParameters {
                    sampling_freq: 0.0,
                    buffer_size: 0
                }
            }
        }
    }


    //  Test output node.  The audio graph should reject any attempts to add output node types into the graph
    pub struct TestOutputNode {
        node_type: AudioNodeType
    }

    impl AudioNode for TestOutputNode {
        fn get_node_type(&self) -> &AudioNodeType {
            &self.node_type
        }

        fn get_number_of_inputs(&self) -> usize {
            0
        }

        fn get_next_available_input(&self) -> Option<usize> {
            None
        }
    }

    impl TestOutputNode {
        pub fn new() -> TestOutputNode {
            TestOutputNode {
                node_type: AudioNodeType::Output
            }
        }
    }

}



mod tests {
    use super::{AudioToolbox, TestNodes};

    #[test]
    fn add_node_to_graph() {
        let mut graph = AudioToolbox::AudioGraph::new();
        let node = Box::new(TestNodes::TestNode::new());
        let id = graph.add_new_node(node).unwrap();

        //  The id of the first node should always be 1 (since 0 is reserved for the output)
        assert_eq!(id, 1);

        let another_node = Box::new(TestNodes::TestNode::new());
        let id_another_node = graph.add_new_node(another_node).unwrap();

        assert_eq!(id_another_node, 2);
    }

    #[test]
    fn connect_nodes_in_graph() {
        let mut graph = AudioToolbox::AudioGraph::new();
        let n1 = Box::new(TestNodes::TestNode::new());
        let n2 = Box::new(TestNodes::TestNode::new());

        //  Attempt to connect non-existent nodes
        let result = graph.connect_node(1, 2, 0);
        match result {
            Err(e) => { println!("{}", e.message); },
            _ => {panic!()}
        }

        //  Attempt to add a node of type Output
        let n0 = Box::new(TestNodes::TestOutputNode::new());
        let result = graph.add_new_node(n0);
        match result {
            None => {},
            _ => { panic!(); }
        }

        let id_n1 = graph.add_new_node(n1).unwrap();
        let id_n2 = graph.add_new_node(n2).unwrap();

        //  Ensure node ids are as expected
        assert_eq!(id_n1, 1);
        assert_eq!(id_n2, 2);

        //  Attempt to connect a node to itself
        let result = graph.connect_node(id_n1, id_n1, 0);
        match result {
            Err(e) => {},
            _ => { panic!(); }
        }

        //  Connect nodes as following:
        //  [n1]->[n2]->[Output]
        let result = graph.connect_node(id_n1, id_n2, 0);
        match result {
            Err(e) => { println!("{}", e.message); panic!(); },
            _ => {}
        }

        let result = graph.connect_node_to_output(id_n2);
        match result {
            Err(e) => { println!("{}", e.message); panic!(); },
            _ => {}
        }

        //  Attempt to connect another node to the output (The output can only accept one child node!)
        let result = graph.connect_node_to_output(id_n1);
        match result {
            Err(e) => { println!("{}", e.message); },
            _ => { panic!(); }
        }

        //  Attempt to connect n1 to n2 again (this should not work as a connection is already established)
        let result = graph.connect_node(id_n1, id_n2, 0);
        match result {
            Err(e) => { println!("{}", e.message); },
            _ => { panic!(); }
        }
    }

    #[test]
    fn run_audio_graph() {
        //  Here, a simple audio graph is made and run.  The expected output is [0.5, 0.5, 0.5, 0.5]
        let mut graph = AudioToolbox::AudioGraph::new();
        let n1 = Box::new(TestNodes::TestGenNode::new());
        let n2 = Box::new(TestNodes::TestFXNode::new());

        let n1_id = graph.add_new_node(n1).unwrap();
        let n2_id = graph.add_new_node(n2).unwrap();

        let result = graph.connect_node(n1_id, n2_id, 0);
        match result {
            Err(e) => { println!("{}", e.message); panic!(); }
            _ => {}
        }

        let result = graph.connect_node_to_output(n2_id);
        match result {
            Err(e) => { println!("{}", e.message); panic!(); }
            _ => {}
        }

        let runtime_params = AudioToolbox::AudioRuntimeParameters {
            sampling_freq: 44_100.0,
            buffer_size: 4
        };

        let result = graph.prepare(runtime_params);
        match result {
            Err(e) => { println!("{}", e.message); panic!(); }
            _ => {}
        }

        let mut buffer = [0.0; 4];
        graph.process_block(&mut buffer);

        assert_eq!(buffer[0], 0.5);
        assert_eq!(buffer[1], 0.5);
        assert_eq!(buffer[2], 0.5);
        assert_eq!(buffer[3], 0.5);
    }
}