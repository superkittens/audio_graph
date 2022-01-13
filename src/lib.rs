

pub mod AudioToolbox {

    pub enum AudioNodeType {
        Test,
        Generator,
        Effect,
        Mixer,
        Output,
        Unknown
    }

    pub struct Error {
        pub code: ErrorCodes,
        pub message: String
    }

    pub enum ErrorCodes {
        node_id_non_existent,
        node_input_port_invalid,
        node_no_more_inputs,
        node_parent_already_exists,
        output_node_has_no_inputs,
        connection_already_exists
    }

    pub trait AudioNode {
        fn init(&mut self) {}
        fn get_node_type(&self) -> &AudioNodeType;
        fn get_number_of_inputs(&self) -> usize;
        fn get_next_available_input(&self) -> Option<usize>;
        fn connect_input(&mut self);
        fn change_parameters<'a>(&mut self, parametres: &'a [f32]) {}
        fn reset(&mut self) {}
        fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] { buffer }
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

    /// An iterator for traversing the audio graph when calling process_block
    /// Traversal is depth-first
    /// An instance of TreeIterator is created for every NodeTree.  As a node gets added, it should push back an element onto the stack
    /// This way, we avoid having to do dynamic memory allocations in the AudioGraph::process_block() call
    /// 
    /// Before traversing the graph, init() must be called before next() is called
    /// The stack stores a tuple containing the id of the next node, the number of children that node has and an index value pointing to which child has been accounted for
    // struct TreeIterator {
    //     stack: Vec<(usize, usize, usize)>,
    //     stack_size: usize
    // }

    // impl TreeIterator {

    //     /// Initialize the iterator with the root node id.  init() will do a first pass traversal and place nodes to visit in the stack
    //     fn init(&mut self, tree: &NodeTree) {
    //         self.go_to_branch_end(tree, 0);
    //     }

    //     /// For a given node, go to the end of its branch and push the intermediate nodes onto the stack
    //     fn go_to_branch_end(&mut self, tree: &NodeTree, node_id: usize) {
    //         let mut current_id = node_id;
    //         loop {
    //             self.stack[self.stack_size] = (current_id, tree.nodes[current_id].children.len(), 0);
    //             self.stack_size += 1;

    //             if tree.nodes[current_id].children.len() > 0 {
    //                 current_id = tree.nodes[current_id].children[0];
    //             } else {
    //                 break;
    //             }
    //         }
    //     }

    //     /// Retrieve the next node id in the audio graph chain.  Will return None if the entire graph has been processed
    //     fn next(&mut self, tree: &NodeTree) -> Option<usize> {
    //         if self.stack_size > 0 {
    //             //  First, check to see if there are any unvisited children nodes in the current node
    //             //  If there are, then traverse to the end of the branch and push the nodes along the way onto the stack
    //             let (node_id, num_children, child_index) = self.stack[self.stack_size - 1];
    //             if child_index + 1 < num_children {
    //                 //  Mark child as visited
    //                 self.stack[self.stack_size].2 = child_index + 1;

    //                 let child_node_id = tree.nodes[node_id].children[child_index + 1];
    //                 self.go_to_branch_end(tree, child_node_id);
    //             }

    //             let next_node_id = self.stack[self.stack_size - 1].0;
    //             self.stack_size -= 1;

    //             return Some(next_node_id);
    //         }
            
    //         None
    //     }
    // }



    /// Audio Graph
    /// An audio graph is responsible for creating and modifying audio samples that eventually are written to some output buffer
    /// Sample creation and modification is handled by the individual nodes 
    /// When get_samples() is called, the graph will traverse through each node, where each node will either place samples (if a generator node) or modify it (effect node)
    pub struct AudioGraph {
        nodes: Vec<Box<dyn AudioNode + 'static>>,
        graph_map: NodeTree,
        iter_stack: Vec<(usize, usize, usize)>,
        iter_stack_size: usize
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
                iter_stack_size: 0
            }
        }

        /// Add an AudioNode to the graph
        /// NOTE that calling this function will NOT establish any connections to other nodes.  It simply adds the node to the ownership list, nodes
        /// This function will return an identification number that the user can then use to reference the added node when making connections/disconnections
        pub fn add_new_node(&mut self, n: Box<dyn AudioNode + 'static>) -> usize {
            self.nodes.push(n);
            self.graph_map.nodes.push(MapNode::new());
            self.iter_stack.push((0, 0, 0));

            self.nodes.len() - 1
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
            if node_in_id > self.nodes.len() || node_out_id > self.nodes.len() {
                return Err(Error{
                    code: ErrorCodes::node_id_non_existent,
                    message: String::from("Node ID does not exist in graph")
                });
            }

            if node_in_input_port >= self.nodes[node_in_id].get_number_of_inputs() {
                return Err(Error {
                    code: ErrorCodes::node_input_port_invalid,
                    message: String::from("Node input port not valid")
                });
            }

            if self.nodes[node_in_id].get_next_available_input() == None {
                return Err(Error {
                    code: ErrorCodes::node_no_more_inputs,
                    message: String::from("Input node has no more available inputs")
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
                });}
            }

            Ok(())
        }

        pub fn process_block<'a>(&mut self, buffer: &'a mut [f32]) -> &'a mut [f32] {
            //  First initialize the stack used for graph traversal
            self.go_to_branch_end(0);

            while let Some(node) = self.next() {
                // self.nodes[node].process_block(buffer);
                println!("Node: {}", node);
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

        // pub fn get_node_mut(&mut self, id: usize) -> Option<&mut Node> {
        //     if id >= self.nodes.len() {
        //         return None
        //     }

        //     Some(&mut self.nodes[id])
        // }

        // pub fn modify_node(&mut self, id: usize) {
        //     self.nodes[id].data = 888;
        // }
    }
}


mod TestNodes {
    use super::AudioToolbox::{AudioNodeType, AudioNode};

    pub struct TestNode {
        data: i32,
        node_type: AudioNodeType,
        num_inputs: usize,
        inputs: [i32; 1],
        next_available_input: usize
    }

    impl AudioNode for TestNode {
        
        fn init(&mut self) {
            self.data = 88;
            self.node_type = AudioNodeType::Test;
        }

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

        fn change_parameters<'a>(&mut self, parametres: &'a [f32]) {

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
}



mod tests {
    use super::{AudioToolbox, TestNodes};

    #[test]
    fn add_node_to_graph() {
        let mut graph = AudioToolbox::AudioGraph::new();
        let node = Box::new(TestNodes::TestNode::new());
        let id = graph.add_new_node(node);

        //  The id of the first node should always be 1 (since 0 is reserved for the output)
        assert_eq!(id, 1);

        let another_node = Box::new(TestNodes::TestNode::new());
        let id_another_node = graph.add_new_node(another_node);

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

        let id_n1 = graph.add_new_node(n1);
        let id_n2 = graph.add_new_node(n2);

        assert_eq!(id_n1, 1);
        assert_eq!(id_n2, 2);

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

        let mut buffer: [f32; 5] = [0.; 5];
        graph.process_block(&mut buffer);
    }
}