import com.automq.elasticstream.client.DefaultStreamClient;
import com.automq.elasticstream.client.api.StreamClient;

public class Main {
    public static void main(String[] args) {
        StreamClient streamClient = new DefaultStreamClient("127.0.0.1:12378");
        System.out.println("It works");
    }
}
