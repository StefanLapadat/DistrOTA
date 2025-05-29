Proof of concept project for distributed online traveling agency (something like Booking.com, Airbnb - but distributed). Idea is to build a system in which traveling agencies would connect peer-to-peer to one another and share the data about properties. 

Currently there are approximately 10 milion properties in the world online (houses, villas, studios etc). Idea here is to build a distributed system which would be able to handle synchronizing changes on all of them (availability, reservations, 
prices etc) for all the hosts in network in the most cost effective way possible. Thinking in lines of being able to host the system in the cloud and operate it for under 200 euros per month while having access to all 10 milion properties. 

Setup

After cloning repository you can use index.js (run by node index.js) script to generate example graph where each of exactly n peers is connected to at least deg peers. Store the output to 'graph.txt' file). 
Use run_instances.sh script to run n instances of application locally to simulate n peers communicating events in the network (prices and availability changes). 

Example with 10 peers: 

![image](https://github.com/user-attachments/assets/541fe214-b3ed-485a-a304-0b0a9c95455e)

The code is setup to simulate each host sending 1200 events (each of them is ~300 bytes message) per second to its peers (including signing messages using private/public keys for integrity checks and fraud prevention). This load corresponds to having 10 milion properties in the system firing an event (reservation made, availability change, price change) aproximatelly once in 3 hours, which would be more than enough for real world scenario. When we run this system on a local machine 8GB ram, Intel(R) Core(TM) i7-8565U CPU @ 1.80GHz, we can see that simulating synchronization changes of whole world properties takes only about 3% of CPU (per host). 

![image](https://github.com/user-attachments/assets/b8962ec0-b35d-4172-8d5c-acfc08c8aa34)

Of course, this is a simplified scenario, and not all operations are supported - the protocol for exchanging property data is not fully developed, but as a proof of concept this project serves the purpose to show that this could be a viable direction to pursue, maybe even revolutionary to the current state of how things are done - having few OTAs like Booking.com and Airbnb that dominate the market and take a large piece from the end price that travelers pay for their rentals (about 20-25% of the final price goes to OTA). 
